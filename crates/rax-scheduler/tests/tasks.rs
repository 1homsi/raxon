//! Task marshaling: priority ordering and cross-thread enqueue onto the UI
//! thread (the R1 thread-confinement gap, closed via the scheduler).

use std::sync::{Arc, Mutex};

use rax_scheduler::*;

#[test]
fn tasks_run_high_then_normal_then_idle_after_phases() {
    let order = Arc::new(Mutex::new(Vec::<&str>::new()));
    let mut s = Scheduler::new(ManualClock::new());
    let spawner = s.spawner();

    // Spawn in mixed order; priority + phase placement decide execution order.
    {
        let order = order.clone();
        spawner.spawn(Priority::Idle, move || order.lock().unwrap().push("idle"));
    }
    {
        let order = order.clone();
        spawner.spawn(Priority::Normal, move || {
            order.lock().unwrap().push("normal")
        });
    }
    {
        let order = order.clone();
        spawner.spawn(Priority::High, move || order.lock().unwrap().push("high"));
    }
    {
        let order = order.clone();
        s.on_phase(Phase::Commit, move |_| order.lock().unwrap().push("commit"));
    }

    assert!(s.needs_frame(), "spawning a task requests a frame");
    s.run_frame();

    // High/Normal drain before phases; Idle runs after the frame's work.
    assert_eq!(
        *order.lock().unwrap(),
        vec!["high", "normal", "commit", "idle"]
    );
}

#[test]
fn task_spawned_from_another_thread_runs_on_the_scheduler_thread() {
    let mut s = Scheduler::new(ManualClock::new());
    let spawner = s.spawner();
    let log = Arc::new(Mutex::new(Vec::<u32>::new()));

    let handle = {
        let spawner = spawner.clone();
        let log = log.clone();
        std::thread::spawn(move || {
            spawner.spawn_normal(move || log.lock().unwrap().push(42));
        })
    };
    handle.join().unwrap();

    assert!(
        s.needs_frame(),
        "cross-thread spawn set the frame-request flag"
    );
    s.run_frame();
    assert_eq!(
        *log.lock().unwrap(),
        vec![42],
        "task executed on the scheduler thread"
    );
}
