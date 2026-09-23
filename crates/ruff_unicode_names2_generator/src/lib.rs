use formatting::Context;
use std::{
    char, cmp,
    collections::{hash_map, HashMap},
    fs::{self, File},
    io::{self, prelude::*, BufWriter},
    iter::repeat,
    path::Path,
};

macro_rules! w {
    ($ctxt: expr, $($tt: tt)*) => {
        (write!($ctxt.out, $($tt)*)).unwrap()
    }
}

mod formatting;
mod phf;
mod trie;
mod util;

const SPLITTERS: &[u8] = b"-";

struct TableData {
    codepoint_names: Vec<(char, &'static str)>,
    cjk_ideograph_ranges: Vec<(char, char)>,
}

fn get_table_data(unicode_data: &'static str) -> TableData {
    fn extract(line: &'static str) -> Option<(char, &'static str)> {
        let splits: Vec<_> = line.splitn(15, ';').collect();
        assert_eq!(splits.len(), 15);
        let s = splits[0];
        let cp = u32::from_str_radix(s, 16)
            .ok()
            .unwrap_or_else(|| panic!("invalid {}", line));
        let c = char::from_u32(cp)?;
        let name = splits[1];
        Some((c, name))
    }

    let mut iter = unicode_data.lines();

    let mut codepoint_names = vec![];
    let mut cjk_ideograph_ranges = vec![];

    while let Some(l) = iter.next() {
        if l.is_empty() {
            break;
        }
        let (cp, name) = if let Some(extracted) = extract(l.trim()) {
            extracted
        } else {
            continue;
        };
        if name.starts_with('<') {
            assert!(name.ends_with('>'), "should >: {}", name);
            let name = &name[1..name.len() - 1];
            if name.starts_with("CJK Ideograph") {
                assert!(name.ends_with("First"));
                // should be CJK Ideograph ..., Last
                let line2 = iter.next().expect("unclosed ideograph range");
                let (cp2, name2) = if let Some(extracted) = extract(line2.trim()) {
                    extracted
                } else {
                    continue;
                };
                assert_eq!(&*name.replace("First", "Last"), &name2[1..name2.len() - 1]);

                cjk_ideograph_ranges.push((cp, cp2));
            } else if name.starts_with("Hangul Syllable") {
                // the main lib only knows this range, so lets make
                // sure we're not going out of date wrt the unicode
                // standard.
                if name.ends_with("First") {
                    assert_eq!(cp, '\u{AC00}');
                } else if name.ends_with("Last") {
                    assert_eq!(cp, '\u{D7A3}');
                } else {
                    panic!("unknown hangul syllable {}", name)
                }
            }
        } else {
            codepoint_names.push((cp, name))
        }
    }
    TableData {
        codepoint_names,
        cjk_ideograph_ranges,
    }
}

pub struct Alias {
    pub code: &'static str,
    pub alias: &'static str,
    pub category: &'static str,
}

pub fn get_aliases(name_aliases: &'static str) -> Vec<Alias> {
    let mut aliases = Vec::new();
    for line in name_aliases.lines() {
        if line.is_empty() | line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ';');
        let code = parts.next().expect(line);
        let alias = parts.next().expect(code);
        let category = parts.next().expect(alias);
        aliases.push(Alias {
            code,
            alias,
            category,
        });
    }
    aliases
}

fn write_cjk_ideograph_ranges(ctxt: &mut Context, ranges: &[(char, char)]) {
    ctxt.write_debugs("CJK_IDEOGRAPH_RANGES", "(char, char)", ranges)
}

/// Construct a huge string storing the text data, and return it,
/// along with information about the position and frequency of the
/// constituent words of the input.
fn create_lexicon_and_offsets(
    mut codepoint_names: Vec<(char, &str)>,
) -> (String, Vec<(usize, Vec<u8>, usize)>) {
    codepoint_names.sort_by(|a, b| a.1.len().cmp(&b.1.len()).reverse());

    // a trie of all the suffixes of the data,
    let mut t = trie::Trie::new();
    let mut output = String::new();

    let mut substring_overlaps = 0;
    let mut substring_o_bytes = 0;

    for &(_, name) in codepoint_names.iter() {
        for n in util::split(name, SPLITTERS) {
            if n.len() == 1 && SPLITTERS.contains(&n.as_bytes()[0]) {
                continue;
            }

            let (already, previous_was_exact) = t.insert(n.bytes(), None, false);
            if already {
                if !previous_was_exact {
                    substring_overlaps += 1;
                    substring_o_bytes += n.len();
                }
            } else {
                // completely new element, i.e. not a substring of
                // anything, so record its position & add it.
                let offset = output.len();
                t.set_offset(n.bytes(), offset);
                output.push_str(n);

                // insert the suffixes of this word which saves about
                // 10KB (we could theoretically insert all substrings,
                // upto a certain length, but this only saves ~300
                // bytes or so and is noticeably slower).
                for i in 1..n.len() {
                    if t.insert(n[i..].bytes(), Some(offset + i), true).0 {
                        // once we've found a string that's already
                        // been inserted, we know all suffixes will've
                        // been inserted too.
                        break;
                    }
                }
            }
        }
    }
    let words: Vec<_> = t
        .iter()
        .map(|(a, b, c)| (a, b, c.expect("unset offset?")))
        .collect();
    println!(
        "Lexicon: # words {}, byte size {}, with {} ({} bytes) non-exact matches",
        words.len(),
        output.len(),
        substring_overlaps,
        substring_o_bytes
    );
    (output, words)
}

// creates arrays t1, t2 and a shift such that `dat[i] == t2[t1[i >>
// shift] << shift + i & mask]`; this allows us to share blocks of
// length `1 << shift`, and so compress an array with a lot of repeats
// (like the 0's of the phrasebook_offsets below).
fn bin_data(dat: &[u32]) -> (Vec<u32>, Vec<u32>, usize) {
    let mut smallest = 0xFFFFFFFF;
    let mut data = (vec![], vec![], 0);
    let mut cache = HashMap::new();

    // brute force search for the shift that words best.
    for shift in 0..14 {
        cache.clear();

        let mut t1 = vec![];
        let mut t2 = vec![];
        for chunk in dat.chunks(1 << shift) {
            // have we stored this chunk already?
            let index = *match cache.entry(chunk) {
                hash_map::Entry::Occupied(o) => o.into_mut(),
                hash_map::Entry::Vacant(v) => {
                    // no :(, better put it in.
                    let index = t2.len();
                    t2.extend(chunk.iter().cloned());
                    v.insert(index)
                }
            };
            t1.push((index >> shift) as u32)
        }

        let my_size = t1.len() * util::smallest_type(t1.iter().copied())
            + t2.len() * util::smallest_type(t2.iter().copied());
        println!("binning: shift {}, size {}", shift, my_size);
        if my_size < smallest {
            data = (t1, t2, shift);
            smallest = my_size
        }
    }

    // verify.
    {
        let (ref t1, ref t2, shift) = data;
        let mask = (1 << shift) - 1;
        for (i, &elem) in dat.iter().enumerate() {
            assert_eq!(
                elem,
                t2[((t1[i >> shift] << shift) + (i as u32 & mask)) as usize]
            )
        }
    }

    data
}

fn write_codepoint_maps(ctxt: &mut Context, codepoint_names: Vec<(char, &str)>) {
    let (lexicon_string, mut lexicon_words) = create_lexicon_and_offsets(codepoint_names.clone());

    let num_escapes = (lexicon_words.len() + 255) / 256;

    // we reserve the high bit (end of word) and 127,126... for
    // non-space splits.  The high bit saves about 10KB, and doing the
    // extra splits reduces the space required even more (e.g. - is a
    // reduction of 14KB).
    let short = 128 - SPLITTERS.len() - num_escapes;

    // find the `short` most common elements
    lexicon_words.sort_by(|a, b| a.cmp(b).reverse());

    // and then sort the rest into groups of equal length, to allow us
    // to avoid storing the full length table; just the indices. The
    // ordering is irrelevant here; just that they are in groups.
    lexicon_words[short..].sort_by_key(|(_, word, _offset)| word.len());

    // the encoding for each word, to avoid having to recompute it
    // each time, we can just blit it out of here.
    let mut word_encodings = HashMap::new();
    for (i, x) in SPLITTERS.iter().enumerate() {
        // precomputed
        word_encodings.insert(vec![*x], vec![128 - 1 - i as u32]);
    }

    // the indices into the main string
    let mut lexicon_offsets = vec![];
    // and their lengths, for the most common strings, since these
    // have no information about their length (they were chosen by
    // frequency).
    let mut lexicon_short_lengths = vec![];
    let mut iter = lexicon_words.into_iter().enumerate();

    for (i, (_, word, offset)) in iter.by_ref().take(short) {
        lexicon_offsets.push(offset);
        lexicon_short_lengths.push(word.len());
        // encoded as a single byte.
        assert!(word_encodings.insert(word, vec![i as u32]).is_none())
    }

    // this stores (end point, length) for each block of words of a
    // given length, where `end point` is one-past-the-end.
    let mut lexicon_ordered_lengths = vec![];
    let mut previous_len = 0xFFFF;
    for (i, (_, word, offset)) in iter {
        let (hi, lo) = (short + i / 256, i % 256);
        assert!(short <= hi && hi < 128 - SPLITTERS.len());
        lexicon_offsets.push(offset);
        let len = word.len();
        if len != previous_len {
            if previous_len != 0xFFFF {
                lexicon_ordered_lengths.push((i, previous_len));
            }
            previous_len = len;
        }

        assert!(word_encodings
            .insert(word, vec![hi as u32, lo as u32])
            .is_none());
    }
    // don't forget the last one.
    lexicon_ordered_lengths.push((lexicon_offsets.len(), previous_len));

    // phrasebook encodes the words out of the lexicon that make each
    // codepoint name.
    let mut phrasebook = vec![0u32];
    // this is a map from `char` -> the index in phrasebook. it is
    // currently huge, but it has a lot of 0's, so we compress it
    // using the binning, below.
    let mut phrasebook_offsets = repeat(0).take(0x10FFFF + 1).collect::<Vec<_>>();
    let mut longest_name = String::new();
    for &(cp, name) in codepoint_names.iter() {
        longest_name = cmp::max_by_key(normalise_name(name, cp), longest_name, |s| s.len());

        let start = phrasebook.len() as u32;
        phrasebook_offsets[cp as usize] = start;

        let mut last_len = 0;
        for w in util::split(name, SPLITTERS) {
            let data = word_encodings
                .get(w.as_bytes())
                .expect(concat!("option on ", line!()));
            last_len = data.len();
            // info!("{}: '{}' {}", name, w, data);

            // blit the data.
            phrasebook.extend(data.iter().cloned())
        }

        // add the high bit to the first byte of the last encoded
        // phrase, to indicate the end.
        let idx = phrasebook.len() - last_len;
        phrasebook[idx] |= 0b1000_0000;
    }

    // compress the offsets, hopefully collapsing all the 0's.
    let (t1, t2, shift) = bin_data(&phrasebook_offsets);

    w!(
        ctxt,
        "pub const LONGEST_NAME: &str = {longest_name:?};\n\
        pub const LONGEST_NAME_LEN: usize = LONGEST_NAME.len();\n"
    );
    ctxt.write_plain_string("LEXICON", &lexicon_string);
    ctxt.write_debugs("LEXICON_OFFSETS", "u32", &lexicon_offsets);
    ctxt.write_debugs("LEXICON_SHORT_LENGTHS", "u8", &lexicon_short_lengths);
    w!(
        ctxt,
        "pub const LEXICON_ORDERED_LENGTHS_LEN: usize = {};\n",
        lexicon_ordered_lengths.len()
    );
    ctxt.write_debugs(
        "LEXICON_ORDERED_LENGTHS",
        "(usize, u8)",
        &lexicon_ordered_lengths,
    );
    w!(ctxt, "pub static PHRASEBOOK_SHORT: u8 = {};\n", short);
    ctxt.write_debugs("PHRASEBOOK", "u8", &phrasebook);
    w!(
        ctxt,
        "pub static PHRASEBOOK_OFFSET_SHIFT: usize = {};\n",
        shift
    );
    ctxt.write_debugs(
        "PHRASEBOOK_OFFSETS1",
        &util::smallest_u(t1.iter().copied()),
        &t1,
    );
    ctxt.write_debugs(
        "PHRASEBOOK_OFFSETS2",
        &util::smallest_u(t2.iter().copied()),
        &t2,
    );
}

fn make_context(path: Option<&Path>) -> Context {
    let mut ctxt = Context {
        out: match path {
            Some(p) => Box::new(BufWriter::new(
                File::create(p.with_extension("tmp")).unwrap(),
            )) as Box<dyn Write>,
            None => Box::new(io::sink()) as Box<dyn Write>,
        },
    };
    let _ = ctxt
        .out
        .write(b"// autogenerated by generator.rs\n")
        .unwrap();
    ctxt
}

#[allow(clippy::type_complexity)]
fn get_truncated_table_data(
    unicode_data: &'static str,
    truncate: Option<usize>,
) -> (Vec<(char, &'static str)>, Vec<(char, char)>) {
    let TableData {
        mut codepoint_names,
        cjk_ideograph_ranges: cjk,
    } = get_table_data(unicode_data);
    if let Some(n) = truncate {
        codepoint_names.truncate(n)
    }
    (codepoint_names, cjk)
}

pub fn generate_phf(
    unicode_data: &'static str,
    path: Option<&Path>,
    truncate: Option<usize>,
    lambda: usize,
    tries: usize,
) {
    let (codepoint_names, _) = get_truncated_table_data(unicode_data, truncate);

    let codepoint_names: Vec<_> = codepoint_names
        .into_iter()
        .map(|(c, s)| (c, normalise_name(s, c)))
        .collect();

    let mut ctxt = make_context(path);
    let (n, disps, data) = phf::create_phf(&codepoint_names, lambda, tries);

    w!(ctxt, "pub static NAME2CODE_N: u64 = {};\n", n);
    ctxt.write_debugs("NAME2CODE_DISP", "(u16, u16)", &disps);

    ctxt.write_debugs("NAME2CODE_CODE", "char", &data);

    if let Some(path) = path {
        fs::rename(path.with_extension("tmp"), path).unwrap()
    }
}

/// Convert a Unicode name to a form that can be used for loose matching, as per
/// [UAX#44](https://www.unicode.org/reports/tr44/tr44-34.html#Matching_Names)
///
/// This function matches `unicode_names2::normalise_name` in implementation, thus the result of one
/// can be used to query a PHF generated from the other.
fn normalise_name(s: &str, codepoint: char) -> String {
    let mut normalised = String::new();
    let bytes = s.as_bytes();
    for (i, c) in bytes.iter().map(u8::to_ascii_uppercase).enumerate() {
        // "Ignore case, whitespace, underscore ('_'), [...]"
        if c.is_ascii_whitespace() || c == b'_' {
            continue;
        }

        // "[...] and all medial hyphens except the hyphen in U+1180 HANGUL JUNGSEONG O-E."
        if codepoint != '\u{1180}' // HANGUL JUNGSEONG O-E
            && c == b'-'
            && bytes.get(i - 1).map_or(false, u8::is_ascii_alphanumeric)
            && bytes.get(i + 1).map_or(false, u8::is_ascii_alphanumeric)
        {
            continue;
        }
        assert!(
            c.is_ascii_alphanumeric() || c == b'-',
            "U+{:04X} contains an invalid character for a Unicode name: {:?}",
            codepoint as u32,
            s
        );

        normalised.push(c as char);
    }

    normalised
}

pub fn generate(unicode_data: &'static str, path: Option<&Path>, truncate: Option<usize>) {
    let (codepoint_names, cjk) = get_truncated_table_data(unicode_data, truncate);
    let mut ctxt = make_context(path);

    write_cjk_ideograph_ranges(&mut ctxt, &cjk);
    let _ = ctxt.out.write(b"\n").unwrap();
    write_codepoint_maps(&mut ctxt, codepoint_names);

    if let Some(path) = path {
        fs::rename(path.with_extension("tmp"), path).unwrap()
    }
}

pub fn generate_aliases(name_aliases: &'static str, path: &Path) {
    let mut aliases = phf_codegen::Map::new();
    for Alias { code, alias, .. } in get_aliases(name_aliases).into_iter() {
        let formatted = format!("'\\u{{{code}}}'");
        aliases.entry(alias, &formatted);
    }
    let aliases = aliases.build().to_string().replace("(\"", "(b\"");
    writeln!(BufWriter::new(File::create(path).unwrap()), "{aliases}",).unwrap();
}
