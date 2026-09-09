use super::read_terminal;
use crate::{
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{CancellationToken, Operation},
    trust::secrets::Secret,
};
use rustix::{
    pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt},
    termios::LocalModes,
};
use std::{
    fs::File,
    io::{Read, Write},
    time::Duration,
};

fn terminal() -> (File, File) {
    let master: File = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)
        .map(File::from)
        .unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let slave: File = ioctl_tiocgptpeer(
        &master,
        OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
    )
    .map(File::from)
    .unwrap();
    rustix::fs::fcntl_setfl(&slave, rustix::fs::OFlags::NONBLOCK).unwrap();
    (master, slave)
}

#[tokio::test]
async fn prompt_hides_secret_and_restores_terminal_after_success_timeout_and_cancellation() {
    for mode in ["success", "timeout", "cancel"] {
        let (master, slave): (File, File) = terminal();
        let monitor: File = slave.try_clone().unwrap();
        let observer: File = monitor.try_clone().unwrap();
        let token: CancellationToken = CancellationToken::new();
        let operation: Operation =
            Operation::new(Deadline::new(Duration::from_secs(2)), token.clone());
        let prompt_task: tokio::task::JoinHandle<Result<Secret, Error>> =
            tokio::spawn(async move { read_terminal(slave, &operation).await });
        let presenter: tokio::task::JoinHandle<(String, File)> =
            tokio::task::spawn_blocking(move || present(master, &observer, &token, mode));
        let (mut output, mut master): (String, File) = presenter.await.unwrap();
        let result: Result<Secret, Error> = prompt_task.await.unwrap();
        match mode {
            "success" => assert_eq!(result.unwrap().expose(), "private-token"),
            "timeout" => assert_eq!(result.unwrap_err().kind, ErrorKind::Timeout),
            _ => assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled),
        }
        assert!(
            rustix::termios::tcgetattr(&monitor)
                .unwrap()
                .local_modes
                .contains(LocalModes::ECHO)
        );
        rustix::fs::fcntl_setfl(&master, rustix::fs::OFlags::NONBLOCK).unwrap();
        let _: std::io::Result<usize> = master.read_to_string(&mut output);
        assert!(!output.contains("private-token"));
    }
}

fn present(
    mut master: File,
    observer: &File,
    token: &CancellationToken,
    mode: &str,
) -> (String, File) {
    rustix::fs::fcntl_setfl(&master, rustix::fs::OFlags::NONBLOCK).unwrap();
    let mut output: String = String::new();
    let mut buffer: [u8; 128] = [0; 128];
    let deadline: std::time::Instant = std::time::Instant::now() + Duration::from_secs(5);
    while !output.ends_with("Credential (hidden): ") {
        assert!(std::time::Instant::now() < deadline, "prompt not displayed");
        match master.read(&mut buffer) {
            Ok(size) => output.push_str(std::str::from_utf8(&buffer[..size]).unwrap()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("{error}"),
        }
    }
    assert!(
        !rustix::termios::tcgetattr(observer)
            .unwrap()
            .local_modes
            .contains(LocalModes::ECHO)
    );
    if mode == "success" {
        master.write_all(b"private-token\n").unwrap();
    } else if mode == "cancel" {
        token.cancel();
    }
    (output, master)
}
