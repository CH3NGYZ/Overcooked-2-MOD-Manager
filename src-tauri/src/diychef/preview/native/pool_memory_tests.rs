use super::*;
use crate::diychef::preview::{
    assembly::Tree,
    cache::{Part, Prepared},
    geometry::Geometry,
    native::reader::{Map, Ref},
};
use crate::models::ChefPreviewBounds;
use std::{collections::HashMap, sync::mpsc};

fn prepared() -> Prepared {
    Prepared {
        parts: vec![Part {
            name: "Head".into(),
            geometry: Geometry {
                vertices: vec![[1.; 8]; 3],
                indices: vec![0, 1, 2],
            },
            texture: Some(vec![5; 4096]),
            scale: [1.; 2],
            offset: [0.; 2],
            material_floats: Default::default(),
        }],
        head_bounds: ChefPreviewBounds {
            min: [0.; 3],
            max: [1.; 3],
        },
        diagnostics: vec![],
    }
}

#[test]
fn returned_leases_keep_seed_objects_but_release_worker_payloads_and_allocations() {
    let pool = Pool::new(1);
    let shared_ref = Ref { file: 0, id: 1 };
    let worker_ref = Ref { file: 0, id: 2 };
    let shared_object = Arc::new(Map::new());
    let shared_tree = Arc::new(Tree {
        nodes: vec![],
        by_ref: HashMap::new(),
    });
    let worker_object = Arc::new(Map::new());
    let worker_tree = Arc::new(Tree {
        nodes: vec![],
        by_ref: HashMap::new(),
    });
    let object_weak = Arc::downgrade(&worker_object);
    let tree_weak = Arc::downgrade(&worker_tree);
    let output = pool
        .run(
            0,
            "same-files".into(),
            || {
                let mut store = Store::test_seed();
                store.objects.insert(shared_ref, shared_object.clone());
                store.trees.insert(shared_ref, shared_tree.clone());
                Ok(store)
            },
            |store| {
                let value = prepared();
                store
                    .custom_models
                    .insert("custom-files".into(), value.clone());
                store.custom_bytes = 4204;
                store.objects.insert(worker_ref, worker_object);
                store.trees.insert(worker_ref, worker_tree);
                Ok(value)
            },
        )
        .unwrap();
    let state = pool.state.lock().unwrap();
    let idle = state.idle.as_ref().unwrap();
    assert!(idle.custom_models.is_empty());
    assert_eq!(idle.custom_models.capacity(), 0);
    assert_eq!(idle.custom_bytes, 0);
    assert_eq!((idle.objects.len(), idle.trees.len()), (1, 1));
    assert!(Arc::ptr_eq(&idle.objects[&shared_ref], &shared_object));
    assert!(Arc::ptr_eq(&idle.trees[&shared_ref], &shared_tree));
    assert!(object_weak.upgrade().is_none() && tree_weak.upgrade().is_none());
    // Returning a scene still preserves its independent payload after trimming.
    assert_eq!(output.parts[0].geometry.vertices, vec![[1.; 8]; 3]);
    assert_eq!(output.parts[0].texture.as_ref().unwrap().len(), 4096);
}

#[test]
fn idle_reclamation_drops_shared_resources_without_invalidating_the_stamp() {
    let pool = Pool::new(1);
    pool.run(
        0,
        "same-files".into(),
        || Ok(Store::test_seed()),
        |_| Ok(()),
    )
    .unwrap();
    let (retired, generation) = {
        let state = pool.state.lock().unwrap();
        (
            Arc::downgrade(state.seed.as_ref().unwrap()),
            state.generation,
        )
    };
    pool.reclaim_after_idle(Duration::ZERO);
    assert!(retired.upgrade().is_none());
    {
        let state = pool.state.lock().unwrap();
        assert!(state.seed.is_none() && state.idle.is_none());
        assert_eq!(state.generation, generation);
        assert_eq!(state.cache_epoch, 0);
        assert_eq!(state.stamp.as_deref(), Some("same-files"));
    }
    pool.run(
        0,
        "same-files".into(),
        || {
            let mut fresh = Store::test_seed();
            fresh.default_template = "rebuilt".into();
            Ok(fresh)
        },
        |store| {
            assert_eq!(store.default_template, "rebuilt");
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn idle_deadline_waits_for_active_operations_and_restarts_after_their_return() {
    let pool = Pool::new(1);
    let gate = super::tests::Gate::default();
    let (entered, received) = mpsc::channel();
    let (reclaimed, completed) = mpsc::channel();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| {
            pool.run(
                0,
                "same-files".into(),
                || Ok(Store::test_seed()),
                |_| {
                    entered.send(()).unwrap();
                    gate.wait();
                    Ok(())
                },
            )
        });
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        let generation = pool.state.lock().unwrap().generation;
        let reclaimer = scope.spawn(|| {
            pool.reclaim_after_idle(Duration::from_millis(60));
            reclaimed.send(()).unwrap();
        });
        let while_active = completed.recv_timeout(Duration::from_millis(80));
        {
            let state = pool.state.lock().unwrap();
            assert_eq!(state.active, 1);
            assert_eq!(state.generation, generation);
            assert!(state.seed.is_some());
        }
        gate.release();
        task.join().unwrap().unwrap();
        assert!(while_active.is_err());
        completed.recv_timeout(Duration::from_secs(2)).unwrap();
        reclaimer.join().unwrap();
        let state = pool.state.lock().unwrap();
        assert_eq!(state.active, 0);
        assert_eq!(state.generation, generation);
        assert!(state.seed.is_none() && state.idle.is_none());
    });
}

#[test]
#[ignore = "requires installed game resources; reads only and reports memory payloads"]
fn real_native_pool_memory_profile() {
    let game = std::path::Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    let paths = super::super::reader::sources(game);
    let stamp = crate::diychef::preview::fingerprint::files(&paths).unwrap();
    let pool = Pool::new(crate::concurrency::workers_from(0));
    let phase = |phase, detail| {
        eprintln!(
            "memory_profile={}",
            serde_json::json!({"pid": std::process::id(), "phase": phase, "detail": detail})
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    phase(
        "before_pool",
        serde_json::json!({"configured_workers": crate::concurrency::workers_from(0)}),
    );
    let gate = super::tests::Gate::default();
    let (ready, prepared) = mpsc::channel();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = ["Chef_Racoon", "Chef_Ghost"]
            .into_iter()
            .map(|name| {
                let (pool, paths, stamp, gate, ready) =
                    (&pool, &paths, &stamp, &gate, ready.clone());
                scope.spawn(move || {
                    pool.run(
                        0,
                        stamp.clone(),
                        || Store::new(paths.clone()),
                        |store| {
                            let tree = super::super::tree(store, name)?;
                            let output = super::super::model_parts(store, &tree, name)?;
                            assert!(output.diagnostics.is_empty());
                            let geometry_bytes = output
                                .parts
                                .iter()
                                .map(|p| {
                                    p.geometry.vertices.capacity() * 32
                                        + p.geometry.indices.capacity() * 4
                                })
                                .sum::<usize>();
                            let png_bytes = output
                                .parts
                                .iter()
                                .map(|p| p.texture.as_ref().map_or(0, Vec::len))
                                .sum::<usize>();
                            let decoded_estimate = output
                                .parts
                                .iter()
                                .map(|p| {
                                    crate::diychef::preview::memory::part(
                                        p.geometry.vertices.len() * 32
                                            + p.geometry.indices.len() * 4,
                                        p.texture.as_deref(),
                                    )
                                })
                                .sum::<usize>();
                            ready
                                .send(serde_json::json!({
                                    "name": name,
                                    "parts": output.parts.len(),
                                    "geometry_capacity_bytes": geometry_bytes,
                                    "encoded_png_bytes": png_bytes,
                                    "frontend_decoded_estimated_bytes": decoded_estimate,
                                    "store": store.memory_profile(),
                                }))
                                .unwrap();
                            gate.wait();
                            Ok(())
                        },
                    )
                })
            })
            .collect();
        let results: Vec<_> = (0..2)
            .map(|_| prepared.recv_timeout(Duration::from_secs(30)).unwrap())
            .collect();
        phase("two_real_prepared", serde_json::json!(results));
        gate.release();
        for task in tasks {
            task.join().unwrap().unwrap();
        }
    });
    let state = pool.state.lock().unwrap();
    phase(
        "after_idle_trim",
        serde_json::json!({
            "seed": state.seed.as_ref().unwrap().memory_profile(),
            "idle": state.idle.as_ref().unwrap().memory_profile(),
            "operation_peak": state.peak,
        }),
    );
    drop(state);
    pool.reclaim_after_idle(Duration::ZERO);
    phase(
        "after_pool_reclaim",
        serde_json::json!({"retained_seed": pool.state.lock().unwrap().seed.is_some()}),
    );
}
