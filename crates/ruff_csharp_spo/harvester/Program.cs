// Roslyn C# -> SPO harvester.
//
// Walks a C# source tree and emits one `ruff_spo_triplet::Triple` per line of
// ndjson: {"s":..,"p":..,"o":..,"f":..,"c":..}. The predicate strings are the
// closed `ruff_spo_triplet::Predicate` vocabulary; `ruff_csharp_spo::load`
// rejects anything outside it.
//
// Usage:
//   dotnet run --project CSharpSpoHarvester.csproj -- <source-root> [out.ndjson]
//
// Scaffold scope: syntax layer only (declarations + names as written). Symbol
// resolution (fully-qualified bases, overrides, attribute binding) is the
// SemanticModel upgrade documented in README.md.

using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

const string ns = "medcare";

// Structural facts harvested from declarations are certain by construction;
// mirror ruff_spo_triplet's "declared/structural" provenance tier.
const double f = 1.0;
const double c = 0.9;

if (args.Length < 1)
{
    Console.Error.WriteLine("usage: csharp-spo-harvest <source-root> [out.ndjson]");
    return 2;
}

var root = args[0];
if (!Directory.Exists(root))
{
    Console.Error.WriteLine($"error: source root not found: {root}");
    return 2;
}

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
                    var msubj = $"{ns}:{name}.{m.Identifier.Text}";
                    triples.Add(new Triple(subj, "has_function", msubj, f, c));
                    triples.Add(new Triple(msubj, "rdf:type", "ogit:Function", f, c));
                    if (m.Modifiers.Any(t => t.IsKind(SyntaxKind.StaticKeyword)))
                    {
                        triples.Add(new Triple(msubj, "is_static", "true", f, c));
                    }
                    break;

                default:
                    break;
            }
        }
    }
}

var json = new JsonSerializerOptions { DefaultIgnoreCondition = JsonIgnoreCondition.Never };
using (var w = args.Length > 1
           ? new StreamWriter(args[1])
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
