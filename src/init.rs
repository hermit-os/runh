use std::time;

pub fn init_container() {
    info!("Init-Process started! Will exit in 10 seconds!");
    std::thread::sleep(time::Duration::from_secs(10));
    info!("Init-Process exiting...");
    todo!("Actually implement the init-process functionality!")
}