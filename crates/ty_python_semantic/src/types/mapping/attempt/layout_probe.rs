use std::cell::{Cell, RefCell};
use std::future::Future;
use std::io::Write;
use std::mem::{align_of_val, size_of_val};

use ruff_db::files::system_path_to_file;
use ty_python_core::ProgramFile;

use super::{AttemptMappingEffects, Mailbox, Queued};
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::mapping::effects::MappingEffects;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, ClassType, GenericAlias, Type, TypeContext, TypeMapping,
};
use ruff_python_ast::PythonVersion;

fn record_layout(
    output: &mut impl Write,
    label: &str,
    includes: &str,
    future: impl Future,
) -> std::io::Result<()> {
    writeln!(
        output,
        "{label}\t{}\t{}\t{includes}",
        size_of_val(&future),
        align_of_val(&future),
    )?;
    drop(future);
    Ok(())
}

#[test]
fn queued_mapping_future_layouts() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/mapping_layout.py", "class Box[T]: ...\n")
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mapping_layout.py")?,
        env.program(&db),
    );
    let origin = global_symbol(&db, file, "Box")
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing generic class"))?;
    let context = origin
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    let variable = context
        .variables(&db)
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing type variable"))?;
    let specialization =
        Specialization::new(&db, context, &[Type::TypeVar(variable)][..], None, None);
    let replacement = Specialization::new(&db, context, &[Type::int_literal(1)][..], None, None);
    let alias = GenericAlias::new(&db, origin, specialization);
    let class = ClassType::Generic(alias);
    let ty = Type::instance(&db, &env, class);
    let Type::NominalInstance(nominal) = ty else {
        anyhow::bail!("missing nominal instance");
    };
    let mapping =
        TypeMapping::ApplySpecialization(ApplySpecialization::specialization(replacement));
    let visitor = ApplyTypeMappingVisitor::new(&env);
    let mailbox = RefCell::new(Mailbox::default());
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let effects = AttemptMappingEffects {
        db: &db,
        last_work: &last_work,
        prefix_copies_requested: &prefix_copies_requested,
        mode: Queued {
            visitor: &visitor,
            mailbox: &mailbox,
        },
    };
    let tcx = TypeContext::default();
    db.clone().clear_salsa_events();

    // These are actual async-function futures with the driver's queued provider. None is
    // polled: operand inference above is excluded, and nested sizes must not be summed.
    let mut output =
        std::io::BufWriter::new(std::fs::File::create("/tmp/ty-mapping-future-layouts.tsv")?);
    writeln!(output, "operation\tbytes\talignment\tincludes")?;
    record_layout(
        &mut output,
        "Type::apply_type_mapping_with",
        "dispatch and all embedded branch futures; excludes push_task's owned-mapping wrapper",
        ty.apply_type_mapping_with(&db, &mapping, tcx, &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "NominalInstanceType::apply_type_mapping_with",
        "nominal branches and embedded ClassType future",
        nominal.apply_type_mapping_with(&db, &mapping, tcx, &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "ClassType::apply_type_mapping_with",
        "class dispatch and embedded GenericAlias future",
        class.apply_type_mapping_with(&db, &mapping, tcx, &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "GenericAlias::apply_type_mapping_with",
        "annotation handling, specialization future, and alias reconstruction",
        alias.apply_type_mapping_with(&db, &mapping, tcx, &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "Specialization::apply_type_mapping_with",
        "actual map_types loop and its authored async callback, plus reconstruction",
        specialization.apply_type_mapping_with(&db, &mapping, &[], &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "BoundTypeVarInstance::apply_type_mapping_with",
        "all TypeVar mapping branches and embedded Self-binding future",
        variable.apply_type_mapping_with(&db, &mapping, &visitor, &effects),
    )?;
    record_layout(
        &mut output,
        "Queued::map_type",
        "child request and mailbox wait; excludes the separately driven child mapper",
        effects.map_type(&db, ty, &mapping, tcx, &visitor),
    )?;
    output.flush()?;

    assert!(db.clone().take_salsa_events().is_empty());
    assert_eq!(last_work.get(), None);
    assert_eq!(prefix_copies_requested.get(), 0);
    let mailbox = mailbox.borrow();
    assert!(mailbox.request.is_none());
    assert!(mailbox.answer.is_none());
    Ok(())
}
