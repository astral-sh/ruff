// Roslyn C# -> SPO harvester.
//
// Walks a C# source tree and emits one `ruff_spo_triplet::Triple` per line of
// ndjson: {"s":..,"p":..,"o":..,"f":..,"c":..}. The predicate strings are the
// closed `ruff_spo_triplet::Predicate` vocabulary; `ruff_csharp_spo::load`
// rejects anything outside it.
//
// Usage:
//   dotnet run --project CSharpSpoHarvester.csproj -- <source-root> [out.ndjson] [flags]
//
// Flags (all optional; defaults reproduce the original EF-Core-flavoured
// behaviour so existing invocations are unaffected):
//   --ns <name>                      IRI namespace prefix (default "csharp")
//   --mutator-names a,b,c            exact persistence-mutator method names
//                                    that make a `calls` fact fire (default:
//                                    the EF Core set below)
//   --mutator-prefixes add_,del_     method-name PREFIXES that also count as
//                                    mutators (default: none) — for bespoke
//                                    ADO.NET DALs with a naming convention
//                                    instead of a fixed method set (e.g.
//                                    a hand-rolled DAL's `add_*`/`del_*`/`set_*`)
//   --mutator-receivers mysql        restrict the `calls` fact to invocations
//                                    whose receiver's last identifier segment
//                                    is in this list (default: none = any
//                                    receiver). `main.mysql.add_x(...)`
//                                    matches receiver `mysql`; a form's own
//                                    `set_Foo(...)` does not.
//
// Scaffold scope: syntax layer only (declarations + names as written). Symbol
// resolution (fully-qualified bases, overrides, attribute binding) is the
// SemanticModel upgrade documented in README.md.

using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

// Structural facts harvested from declarations are certain by construction;
// mirror ruff_spo_triplet's "declared/structural" provenance tier.
const double f = 1.0;
const double c = 0.9;

// Original scaffold's closed EF-Core persistence-mutator set — kept as the
// default `--mutator-names` so invocations with no flags are unchanged.
var defaultMutatorNames = new[]
{
    "SaveChanges", "SaveChangesAsync", "Update", "UpdateRange",
    "Add", "AddRange", "Remove", "RemoveRange", "Delete",
    "ExecuteDelete", "ExecuteUpdate",
};

var ns = "csharp";
var mutatorNames = new HashSet<string>(defaultMutatorNames, StringComparer.Ordinal);
var mutatorPrefixes = new List<string>();
var mutatorReceivers = new List<string>();
var positional = new List<string>();

for (var i = 0; i < args.Length; i++)
{
    switch (args[i])
    {
        case "--ns" when i + 1 < args.Length:
            ns = args[++i];
            break;
        case "--mutator-names" when i + 1 < args.Length:
            mutatorNames = new HashSet<string>(SplitCsv(args[++i]), StringComparer.Ordinal);
            break;
        case "--mutator-prefixes" when i + 1 < args.Length:
            mutatorPrefixes = SplitCsv(args[++i]);
            break;
        case "--mutator-receivers" when i + 1 < args.Length:
            mutatorReceivers = SplitCsv(args[++i]);
            break;
        default:
            positional.Add(args[i]);
            break;
    }
}

if (positional.Count < 1)
{
    Console.Error.WriteLine(
        "usage: csharp-spo-harvest <source-root> [out.ndjson] " +
        "[--ns <name>] [--mutator-names a,b,c] [--mutator-prefixes add_,del_] " +
        "[--mutator-receivers mysql]");
    return 2;
}

var root = positional[0];
if (!Directory.Exists(root))
{
    Console.Error.WriteLine($"error: source root not found: {root}");
    return 2;
}

// The one `calls`-fact predicate for this run, closed over the configured
// name set / prefixes / receivers — built once, passed down explicitly
// (EmitBodyArm is a `static` local function and cannot capture top-level
// variables).
var isMutatorCall = BuildMutatorPredicate(mutatorNames, mutatorPrefixes, mutatorReceivers);

var triples = new List<Triple>();

foreach (var file in Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories))
{
    SyntaxNode rootNode;
    try
    {
        rootNode = CSharpSyntaxTree.ParseText(File.ReadAllText(file)).GetRoot();
    }
    catch (IOException ex)
    {
        Console.Error.WriteLine($"skip {file}: {ex.Message}");
        continue;
    }

    // class / struct / record / interface declarations all share TypeDeclarationSyntax.
    foreach (var type in rootNode.DescendantNodes().OfType<TypeDeclarationSyntax>())
    {
        var name = type.Identifier.Text;
        var subj = $"{ns}:{name}";

        // (Class, rdf:type, ogit:ObjectType) — structural classification.
        triples.Add(new Triple(subj, "rdf:type", "ogit:ObjectType", f, c));

        // (Class, inherits_from, Base) for each base type / interface, name as written.
        if (type.BaseList is not null)
        {
            foreach (var b in type.BaseList.Types)
            {
                triples.Add(new Triple(subj, "inherits_from", $"{ns}:{BareName(b.Type)}", f, c));
            }
        }

        foreach (var member in type.Members)
        {
            switch (member)
            {
                case PropertyDeclarationSyntax p:
                    EmitField(triples, ns, subj, name, p.Identifier.Text, p.Type, f, c);
                    break;

                case FieldDeclarationSyntax fd:
                    foreach (var v in fd.Declaration.Variables)
                    {
                        EmitField(triples, ns, subj, name, v.Identifier.Text, fd.Declaration.Type, f, c);
                    }
                    break;

                case MethodDeclarationSyntax m:
                    {
                        var msubj = $"{ns}:{name}.{m.Identifier.Text}";
                        triples.Add(new Triple(subj, "has_function", msubj, f, c));
                        triples.Add(new Triple(msubj, "rdf:type", "ogit:Function", f, c));
                        if (m.Modifiers.Any(t => t.IsKind(SyntaxKind.StaticKeyword)))
                        {
                            triples.Add(new Triple(msubj, "is_static", "true", f, c));
                        }

                        // AST-DLL signature plane (returns_type / has_param_type /
                        // has_visibility) — mirrors the C++ frontend's cpp_method
                        // (ruff_spo_triplet::expand.rs cpp_method) so the codegen's
                        // "helpers/private split" (fuzzy-recipe-codebook.md §2) and
                        // the AST-DLL signature shape work identically for C#.
                        if (m.ReturnType.ToString() != "void")
                        {
                            triples.Add(new Triple(msubj, "returns_type", m.ReturnType.ToString(), f, c));
                        }
                        var parameters = m.ParameterList.Parameters;
                        for (var pi = 0; pi < parameters.Count; pi++)
                        {
                            if (parameters[pi].Type is { } pType)
                            {
                                triples.Add(new Triple(msubj, "has_param_type", $"{pi}:{pType}", f, c));
                            }
                        }
                        // Always present — every method has a visibility, matching
                        // the C++ frontend's "always present" access specifier.
                        triples.Add(new Triple(
                            msubj,
                            "has_visibility",
                            VisibilityOf(m.Modifiers, type is InterfaceDeclarationSyntax),
                            f,
                            c));

                        // DTO ARM (syntax-only) — the body-fact fingerprint
                        // the fuzzy recipe-codebook needs (ruff/.claude/
                        // knowledge/fuzzy-recipe-codebook.md §2). Populates
                        // writes_field / reads_field / raises / calls /
                        // writes_if_blank for a C# method body, so the same
                        // recipe centroids that classify Rails hooks classify
                        // C# `OnSaving`/`SaveChanges` overrides + property
                        // setters, and bespoke ADO.NET DALs (`add_*`/`del_*`
                        // naming, no ORM) via
                        // --mutator-prefixes/--mutator-receivers. TESTED:
                        // built + run with dotnet-sdk-8 against
                        // `fixtures/recipe_shapes.cs` (all 7 recipe centroids
                        // classify correctly, `ruff_csharp_spo` unit tests
                        // round-trip the emitted predicates) AND end-to-end
                        // against a real production C# corpus (~97k triples,
                        // `ruff_csharp_spo::load` validates all of them).
                        EmitBodyArm(triples, ns, name, msubj, m.Body, m.ExpressionBody, f, c, isMutatorCall);
                        break;
                    }

                default:
                    break;
            }
        }
    }
}

var json = new JsonSerializerOptions { DefaultIgnoreCondition = JsonIgnoreCondition.Never };
using (var w = positional.Count > 1
           ? new StreamWriter(positional[1])
           : new StreamWriter(Console.OpenStandardOutput()))
{
    foreach (var t in triples)
    {
        w.WriteLine(JsonSerializer.Serialize(t, json));
    }
}

Console.Error.WriteLine($"harvested {triples.Count} triples from {root}");
return 0;

// (Class, has_field, Class.field) + (Class.field, rdf:type, ogit:Property)
//                                 + (Class.field, field_type, <Type as written>)
static void EmitField(
    List<Triple> triples,
    string ns,
    string classSubj,
    string className,
    string field,
    TypeSyntax type,
    double f,
    double c)
{
    var fsubj = $"{ns}:{className}.{field}";
    triples.Add(new Triple(classSubj, "has_field", fsubj, f, c));
    triples.Add(new Triple(fsubj, "rdf:type", "ogit:Property", f, c));
    triples.Add(new Triple(fsubj, "field_type", type.ToString(), f, c));
}

// DTO ARM (syntax-only) — the body-fact fingerprint for the fuzzy
// recipe-codebook (ruff/.claude/knowledge/fuzzy-recipe-codebook.md §2). Emits,
// for one C# method body, the SAME predicates the Ruby frontend emits so the
// recipe centroids are language-agnostic:
//   writes_field    `this.X = …` / `X = …`     assignment to a member
//   reads_field     `this.X` / bare `X`         member read
//   raises          `throw new XException(…)`   abort signal
//   calls           `ctx.SaveChanges()` / …     persistence-mutator dispatch
//   writes_if_blank `X ??= v` / `if (X==null) X = v`   J1 default-vs-normalize
// Syntax-only, no SemanticModel: a bare `X` is heuristically a member read
// (Inferred, matching Ruby's convention) — a SemanticModel upgrade would prune
// locals/params. `isMutatorCall` is the configured persistence-mutator
// predicate (`--mutator-names`/`--mutator-prefixes`/`--mutator-receivers`,
// see `BuildMutatorPredicate`) — the C# analogue of Ruby's closed
// AR_MUTATORS, made agnostic to ORM vs bespoke ADO.NET DAL naming.
static void EmitBodyArm(
    List<Triple> triples,
    string ns,
    string className,
    string msubj,
    BlockSyntax? body,
    ArrowExpressionClauseSyntax? expressionBody,
    double f,
    double c,
    Func<MemberAccessExpressionSyntax, bool> isMutatorCall)
{
    // Unify block-bodied (`{ … }`) and expression-bodied (`=> …`) methods:
    // walk whichever node holds the body.
    SyntaxNode? root = (SyntaxNode?)body ?? expressionBody?.Expression;
    if (root is null)
    {
        return;
    }

    // LHS of an assignment -> the member name it writes, or null if the LHS is
    // not a plain member (`this.X` / `X`). Indexers, tuples, locals -> null.
    static string? WrittenMember(ExpressionSyntax lhs) => lhs switch
    {
        // `this.X = …`
        MemberAccessExpressionSyntax ma
            when ma.Expression is ThisExpressionSyntax => ma.Name.Identifier.Text,
        // `X = …` (bare — may be a local; SemanticModel would confirm it is a
        // member. Syntax-only keeps it, matching Ruby's bare-attr convention.)
        IdentifierNameSyntax id => id.Identifier.Text,
        _ => null,
    };

    foreach (var node in root.DescendantNodesAndSelf())
    {
        switch (node)
        {
            // ── writes_field + J1 writes_if_blank ──
            case AssignmentExpressionSyntax asgn:
                var w = WrittenMember(asgn.Left);
                if (w is not null)
                {
                    triples.Add(new Triple(msubj, "writes_field", $"{ns}:{className}.{w}", f, c));
                    // J1: `X ??= v` is the null-coalescing default (the C#
                    // spelling of Ruby `x ||= v` / `x = v if x.blank?`).
                    if (asgn.OperatorToken.IsKind(SyntaxKind.QuestionQuestionEqualsToken))
                    {
                        triples.Add(new Triple(msubj, "writes_if_blank", $"{ns}:{className}.{w}", f, c));
                    }
                }
                break;

            // ── J1 writes_if_blank via the `if (X == null) X = v` guard ──
            case IfStatementSyntax ifs
                when NullGuardedField(ifs.Condition) is string gf:
                // Any write to `gf` inside the guarded branch is a default.
                foreach (var inner in ifs.Statement.DescendantNodesAndSelf()
                             .OfType<AssignmentExpressionSyntax>())
                {
                    if (WrittenMember(inner.Left) == gf)
                    {
                        triples.Add(new Triple(msubj, "writes_if_blank", $"{ns}:{className}.{gf}", f, c));
                    }
                }
                break;

            // ── raises ──
            case ThrowStatementSyntax { Expression: ObjectCreationExpressionSyntax oce }:
                triples.Add(new Triple(msubj, "raises", $"exc:{BareName(oce.Type)}", f, c));
                break;
            case ThrowExpressionSyntax { Expression: ObjectCreationExpressionSyntax oce2 }:
                triples.Add(new Triple(msubj, "raises", $"exc:{BareName(oce2.Type)}", f, c));
                break;

            // ── calls (persistence mutators only) ──
            case InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax mac }
                when isMutatorCall(mac):
                // "receiver.method" verbatim, like Ruby's calls object.
                triples.Add(new Triple(msubj, "calls", $"{mac.Expression}.{mac.Name.Identifier.Text}", f, c));
                break;

            // ── reads_field (`this.X` member reads) ──
            // EXCLUDE the assignment LHS: `this.X = …` — the LHS `this.X` is
            // also a MemberAccess node, but it is a WRITE, not a read. Counting
            // it as a read would make `this.X = f(y)` look like `W ⊆ R` (a
            // SelfMap) when it is a Compute — the dangerous over-read direction.
            case MemberAccessExpressionSyntax { Expression: ThisExpressionSyntax } thisRead
                when !(thisRead.Parent is AssignmentExpressionSyntax pa && pa.Left == thisRead):
                triples.Add(new Triple(msubj, "reads_field", $"{ns}:{className}.{thisRead.Name.Identifier.Text}", f, c));
                break;
        }
    }
}

// J1 helper — `X == null` / `X is null` -> the tested member `X`; else null.
// The C# analogue of Ruby's `self.X.blank?`/`.nil?` guard.
static string? NullGuardedField(ExpressionSyntax cond)
{
    static string? MemberName(ExpressionSyntax e) => e switch
    {
        MemberAccessExpressionSyntax ma when ma.Expression is ThisExpressionSyntax
            => ma.Name.Identifier.Text,
        IdentifierNameSyntax id => id.Identifier.Text,
        _ => null,
    };
    return cond switch
    {
        // `X == null`
        BinaryExpressionSyntax be when be.OperatorToken.IsKind(SyntaxKind.EqualsEqualsToken)
            && be.Right is LiteralExpressionSyntax { RawKind: (int)SyntaxKind.NullLiteralExpression }
            => MemberName(be.Left),
        // `X is null`
        IsPatternExpressionSyntax ip when ip.Pattern is ConstantPatternSyntax
            { Expression: LiteralExpressionSyntax { RawKind: (int)SyntaxKind.NullLiteralExpression } }
            => MemberName(ip.Expression),
        // `string.IsNullOrEmpty(X)` / `string.IsNullOrWhiteSpace(X)`
        InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax ma } inv
            when ma.Name.Identifier.Text is "IsNullOrEmpty" or "IsNullOrWhiteSpace"
            && inv.ArgumentList.Arguments.Count == 1
            => MemberName(inv.ArgumentList.Arguments[0].Expression),
        _ => null,
    };
}

// Split a `--flag a,b,c` CLI value into a trimmed, empty-entry-free list.
static List<string> SplitCsv(string s) =>
    s.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).ToList();

// The receiver of an invocation, reduced to its last identifier segment, so
// `main.mysql.add_x(...)` matches `--mutator-receivers mysql` (the `.mysql`
// leg), not the whole `main.mysql` chain. `ctx.SaveChanges()` -> "ctx";
// `this.mysql.add_x(...)` -> "mysql" (the `this.mysql` receiver is itself a
// MemberAccess whose Name is "mysql").
static string ReceiverLastSegment(ExpressionSyntax receiver) => receiver switch
{
    MemberAccessExpressionSyntax ma => ma.Name.Identifier.Text,
    IdentifierNameSyntax id => id.Identifier.Text,
    ThisExpressionSyntax => "this",
    _ => receiver.ToString(),
};

// Builds the one `calls`-fact predicate for a harvest run from the
// `--mutator-names`/`--mutator-prefixes`/`--mutator-receivers` configuration.
// A name match is `mutatorNames.Contains(name)` (exact, the original EF-Core
// behaviour) OR a `mutatorPrefixes` prefix match (for naming-convention DALs
// with `add_*`/`del_*`-style names) — either is sufficient. When
// `mutatorReceivers` is non-empty, the match is further restricted to
// invocations whose receiver's last segment (see `ReceiverLastSegment`) is in
// that list; an empty `mutatorReceivers` matches any receiver (the original
// behaviour, e.g. `ctx.SaveChanges()` with no receiver filter at all).
static Func<MemberAccessExpressionSyntax, bool> BuildMutatorPredicate(
    HashSet<string> mutatorNames,
    List<string> mutatorPrefixes,
    List<string> mutatorReceivers) => mac =>
{
    var name = mac.Name.Identifier.Text;
    var nameMatches = mutatorNames.Contains(name)
        || mutatorPrefixes.Any(p => name.StartsWith(p, StringComparison.Ordinal));
    if (!nameMatches)
    {
        return false;
    }
    return mutatorReceivers.Count == 0
        || mutatorReceivers.Contains(ReceiverLastSegment(mac.Expression));
};

// Method access specifier -> "public"|"protected"|"private", the closed
// `has_visibility` vocabulary (`ruff_spo_triplet::Predicate::HasVisibility`,
// mirroring the C++ frontend's `CppAccess`). C# has no explicit keyword for
// the common "no modifier" case, so the two language defaults are applied:
// `private` for class/struct/record members, *implicitly* `public` for
// interface members. `internal`-only members (package-visible, no C#
// equivalent in the closed three-value vocabulary) collapse to "private":
// per the codebook's API-surface signal (fuzzy-recipe-codebook.md §2 —
// "public methods are the adapter surface; private/protected are likely
// internal"), an internal-only member is not on the public adapter surface
// either, so folding it into "private" preserves that signal even though C#
// `internal` and `private` are technically distinct accessibilities.
// `protected internal` / `private protected` collapse to "protected" (the
// first matching, more-restrictive keyword below wins).
static string VisibilityOf(SyntaxTokenList modifiers, bool isInterfaceMember)
{
    if (modifiers.Any(t => t.IsKind(SyntaxKind.PublicKeyword)))
    {
        return "public";
    }
    if (modifiers.Any(t => t.IsKind(SyntaxKind.ProtectedKeyword)))
    {
        return "protected";
    }
    if (modifiers.Any(t => t.IsKind(SyntaxKind.PrivateKeyword)))
    {
        return "private";
    }
    if (modifiers.Any(t => t.IsKind(SyntaxKind.InternalKeyword)))
    {
        return "private";
    }
    return isInterfaceMember ? "public" : "private";
}

// Base/type name as written, generics stripped (`List<Foo>` -> `List`) so the
// object IRI is a stable class reference. Full generic + symbol resolution is
// the SemanticModel upgrade (README.md).
static string BareName(TypeSyntax type)
{
    var s = type.ToString();
    var lt = s.IndexOf('<');
    return lt >= 0 ? s[..lt] : s;
}

// Mirrors ruff_spo_triplet::Triple field-for-field; the JSON keys are exactly
// s/p/o/f/c so `from_ndjson` deserializes it with no transform.
internal sealed record Triple(
    [property: JsonPropertyName("s")] string S,
    [property: JsonPropertyName("p")] string P,
    [property: JsonPropertyName("o")] string O,
    [property: JsonPropertyName("f")] double F,
    [property: JsonPropertyName("c")] double C);
