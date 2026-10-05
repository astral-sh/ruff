use std::collections::VecDeque;
use std::fmt::Write;

use super::*;
use crate::types::callable::scheduled_probe::mro::{PreparedMroWork, PreparedStaticMroEffects};
use crate::types::mro::Mro;
use crate::types::mro::construction::StaticMroEffects;

#[derive(Debug, Eq, PartialEq)]
enum CollectedMro<'db> {
    Base(VecDeque<ClassBase<'db>>),
    SingleBase(Mro<'db>),
}

#[test]
fn prepared_base_collection_work_scales_linearly_with_tail_length() -> anyhow::Result<()> {
    let mut base_work = Vec::new();
    let mut single_base_work = Vec::new();
    for tail_len in [128, 256] {
        let mut source = "class C0: ...\n".to_owned();
        for index in 1..tail_len - 1 {
            writeln!(source, "class C{index}(C{}): ...", index - 1)?;
        }
        let base_name = format!("C{}", tail_len - 2);
        writeln!(source, "class Root({base_name}): ...")?;
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/mro_collection.py", &source)
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro_collection.py")?,
            env.program(&db),
        );

        // Preparation retains the complete ordinary MRO tails before collection is captured.
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let root = class_named(&prepared, "Root");
        let base = class_named(&prepared, &base_name);
        assert_eq!(prepared.context(base).unwrap(), None);
        assert_eq!(prepared.proper_mro(base).unwrap().len(), tail_len - 1);

        for (with_root, work_by_length) in [(false, &mut base_work), (true, &mut single_base_work)]
        {
            let mut repeated_work = [0; 3];
            for work in &mut repeated_work {
                let router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
                let observed = probe::capture(&db, || {
                    run_with(
                        &db,
                        &env,
                        &router,
                        10_000_000,
                        false,
                        false,
                        |router| async {
                            let work = PreparedMroWork::consumer(router);
                            let effects = PreparedStaticMroEffects::new(&db, &env, &work);
                            let base = ClassBase::Class(ClassType::NonGeneric(base.into()));
                            if with_root {
                                effects
                                    .collect_single_base_mro(
                                        &env,
                                        ClassType::NonGeneric(root.into()),
                                        base,
                                        None,
                                    )
                                    .await
                                    .map(CollectedMro::SingleBase)
                            } else {
                                effects
                                    .collect_base_mro(&env, base, None)
                                    .await
                                    .map(CollectedMro::Base)
                            }
                        },
                    )
                    .unwrap()
                })
                .unwrap();
                assert!(
                    observed.reads.is_empty(),
                    "source reads: {:?}",
                    observed.reads
                );
                assert!(!router.consumer_active.get());
                assert!(!observed.value.graph.exhausted);
                assert!(observed.value.static_mro_starts.is_empty());
                assert!(observed.value.graph.static_mro_pending.is_empty());
                assert!(observed.value.graph.mapping_pending.is_empty());

                // Compare every entry after capture, including the root for single-base MROs.
                let expected = if with_root {
                    let entries: Mro<'_> = root.iter_mro(&db, None).collect();
                    assert_eq!(entries.len(), tail_len + 1);
                    CollectedMro::SingleBase(entries)
                } else {
                    let entries: VecDeque<_> = base.iter_mro(&db, None).collect();
                    assert_eq!(entries.len(), tail_len);
                    CollectedMro::Base(entries)
                };
                assert_eq!(observed.value.consumer, Some(Ok(expected)));
                *work = observed.value.graph.work;
            }
            assert_eq!(repeated_work, [repeated_work[0]; 3]);
            work_by_length.push(repeated_work[0]);
            eprintln!(
                "prepared base collection: tail_len={tail_len}, with_root={with_root}, work={repeated_work:?}"
            );
        }
    }

    // Fixed setup and allocation growth affect the exact ratio; copying twice as many entries
    // still admits approximately twice the work for each collector.
    for work in [base_work, single_base_work] {
        assert!(work[1] * 4 >= work[0] * 7, "work: {work:?}");
        assert!(work[1] * 4 <= work[0] * 9, "work: {work:?}");
    }
    Ok(())
}
