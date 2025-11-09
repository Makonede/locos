use crate::{
    println,
    tasks::scheduler::exit_task,
};

#[test_case]
fn test_multitasking() {
    for _ in 0..5 {
        kcreate_task(do_something, "do something");
        kcreate_task(do_something_else, "do something else");
    }
}

fn do_something() -> ! {
    for i in 0..1000 {
        println!("iteration {}", i);
    }

    exit_task();
}

fn do_something_else() -> ! {
    for i in 0..1000 {
        println!("iteration from 2nd thread {}", i);
    }

    exit_task();
}
