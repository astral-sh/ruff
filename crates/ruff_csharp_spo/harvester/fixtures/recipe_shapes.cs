// Fixture for the C# body-fact arm (EmitBodyArm in ../Program.cs) and the
// signature plane (returns_type / has_param_type / has_visibility). Exercises
// every recipe centroid from ruff/.claude/knowledge/fuzzy-recipe-codebook.md
// §3 (Default via `??=`, Default via `if (x == null)`, Normalize, Compute,
// Cascade, Guard, Compensate, WriteRaise) plus the --mutator-prefixes /
// --mutator-receivers configurability added for bespoke ADO.NET DALs (the
// `mysql.add_x(...)` shape such DALs use instead of an EF Core `SaveChanges`).
//
// Kept deliberately compilable (not just syntactically parseable) so it
// reads as a legitimate C# file, though the harvester itself only ever runs
// `CSharpSyntaxTree.ParseText` and never needs it to build.

using System;

namespace Fixture
{
    public interface IThing
    {
        // No modifier: interface member default visibility is `public`.
        void DoThing();
    }

    public class Base
    {
    }

    // Stand-in for a bespoke ADO.NET DAL: mutators are named by convention
    // (`add_*`/`del_*`), not a fixed EF Core method set — this is exactly
    // what --mutator-prefixes/--mutator-receivers is for.
    public class MysqlDal
    {
        public bool add_x() => true;
        public bool del_x() => true;
    }

    // Stand-in for an EF Core DbContext: SaveChanges is in the harvester's
    // default --mutator-names set, so this one is caught with no flags.
    public class DbContextLike
    {
        public void SaveChanges()
        {
        }
    }

    // Stand-in for the real corpus's `main.mysql.add_x(...)` receiver chain
    // (MainForm.mysql, called as `main.mysql.add_x(...)` from elsewhere).
    public class MainRef
    {
        public MysqlDal mysql = new MysqlDal();
    }

    public class Widget : Base, IThing
    {
        public string Name { get; set; }
        public object Display;
        private int _count;

        private DbContextLike ctx = new DbContextLike();
        private MainRef main = new MainRef();

        // Default (J1, writes_if_blank via `??=`): W == guarded_writes == {Name}.
        public void SetDefaults()
        {
            this.Name ??= "unknown";
        }

        // Default (J1, writes_if_blank via `if (x == null) x = v`): same shape,
        // different spelling.
        public void Backfill()
        {
            if (this.Name == null)
            {
                this.Name = "backfilled";
            }
        }

        // Normalize: W == {Name}, R == {Name}, unconditional (W subseteq R).
        public void Tidy()
        {
            this.Name = this.Name.Trim();
        }

        // Compute: writes a field (Display) it never reads elsewhere in the
        // body (W == {Display}, R == {Name, _count}, Display not in R).
        public void ComputeDisplay()
        {
            this.Display = this.Name + " (" + this._count + ")";
        }

        // Cascade: mutator dispatch only (C, not X) via the EF-style default
        // mutator name `SaveChanges` — fires with NO --mutator-* flags.
        public void Cascade()
        {
            this.ctx.SaveChanges();
        }

        // Same Cascade shape via the bespoke ADO.NET naming convention — only
        // fires a `calls` fact when --mutator-prefixes/--mutator-receivers
        // configure `add_`/`mysql`, since `add_x` is not in the default
        // --mutator-names set.
        public void AddViaMain()
        {
            this.main.mysql.add_x();
        }

        // Guard: abort only (X, not W, not C).
        public void Guard()
        {
            if (this.Name == null)
            {
                throw new ArgumentException("Name required");
            }
        }

        // Compensate: write + call + raise (C and X both true — checked
        // before Cascade/WriteRaise, so it wins even though it also writes).
        public void Compensate()
        {
            this._count = 0;
            this.ctx.SaveChanges();
            throw new InvalidOperationException("rolled back");
        }

        // WriteRaise: write + raise, no call (W and X, but not C).
        public void WriteRaise()
        {
            this._count = -1;
            throw new InvalidOperationException("partial write");
        }

        // Signature plane: non-void return + two typed params + explicit
        // `private` visibility.
        private int Helper(int x, string y)
        {
            return x + y.Length;
        }

        // Signature plane: `protected`, void (returns_type NOT emitted), no
        // params.
        protected virtual void OnSaving()
        {
        }

        // Signature plane: no modifier -> class-member default is `private`.
        void InternalHook()
        {
        }

        // Signature plane: static, expression-bodied, two typed params,
        // non-void return. Empty body facts (a pure arithmetic expression
        // touches no field).
        public static int Sum(int a, int b) => a + b;

        public void DoThing()
        {
        }
    }
}
