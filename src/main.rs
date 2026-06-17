mod ccrl;
mod config;
mod discord;
mod log;
mod state;
mod tcec;

fn main() {
    let ccrl = std::thread::Builder::new()
        .name("ccrl".into())
        .spawn(|| {
            if let Err(e) = std::panic::catch_unwind(|| {
                let _ = ccrl::run();
            }) {
                eprintln!("[ccrl] panic: {:?}", e);
            }
        })
        .unwrap();

    let tcec = std::thread::Builder::new()
        .name("tcec".into())
        .spawn(|| {
            if let Err(e) = std::panic::catch_unwind(|| {
                let _ = tcec::run();
            }) {
                eprintln!("[tcec] panic: {:?}", e);
            }
        })
        .unwrap();

    let _ = ccrl.join();
    let _ = tcec.join();
}
