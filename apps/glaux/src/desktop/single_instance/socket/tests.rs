use super::*;
use crate::scratch_dir::ScratchDir;
use pretty_assertions::assert_eq;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

fn launch(arg: &str) -> SecondaryLaunch {
    SecondaryLaunch {
        args: vec!["glaux".into(), arg.into()],
        cwd: PathBuf::from("/tmp/cartridges"),
    }
}

fn primary(path: &Path) -> (Owner, Receiver<SecondaryLaunch>) {
    let (sender, receiver) = mpsc::channel();
    match claim(path, &launch("primary"), sender) {
        Claim::Owner(owner) => (owner, receiver),
        _ => panic!("expected to own {}", path.display()),
    }
}

#[test]
fn second_launch_is_forwarded_to_the_first() {
    let dir = ScratchDir::new("si-forward");
    let path = dir.join(file_name());
    let (owner, launches) = primary(&path);

    let (sender, _) = mpsc::channel();
    assert!(matches!(
        claim(&path, &launch("pong"), sender),
        Claim::Forwarded
    ));

    let received = launches.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(received, launch("pong"));
    drop(owner);
}

#[test]
fn stale_socket_is_replaced() {
    let dir = ScratchDir::new("si-stale");
    let path = dir.join(file_name());
    // A bound socket whose owner went away without removing the file,
    // as after a crash.
    drop(UnixListener::bind(&path).unwrap());
    assert!(path.exists());

    let (owner, _launches) = primary(&path);

    drop(owner);
    assert!(!path.exists());
}

#[test]
fn drop_stops_the_listener_and_removes_the_socket() {
    let dir = ScratchDir::new("si-drop");
    let path = dir.join(file_name());
    let (owner, launches) = primary(&path);

    let started = Instant::now();
    drop(owner);

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!path.exists());
    // The listener has exited and dropped its sender.
    assert!(launches.recv_timeout(Duration::from_secs(1)).is_err());
}

#[test]
fn drop_leaves_a_replaced_socket_alone() {
    let dir = ScratchDir::new("si-replaced");
    let path = dir.join(file_name());
    let (owner, _launches) = primary(&path);

    fs::remove_file(&path).unwrap();
    let _other = UnixListener::bind(&path).unwrap();
    drop(owner);

    assert!(path.exists());
}

#[test]
fn garbage_payload_does_not_stop_the_listener() {
    let dir = ScratchDir::new("si-garbage");
    let path = dir.join(file_name());
    let (owner, launches) = primary(&path);

    let mut stream = UnixStream::connect(&path).unwrap();
    stream.write_all(b"definitely not json").unwrap();
    drop(stream);
    let (sender, _) = mpsc::channel();
    assert!(matches!(
        claim(&path, &launch("after"), sender),
        Claim::Forwarded
    ));

    let received = launches.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(received, launch("after"));
    drop(owner);
}

#[test]
fn a_launch_is_read_after_its_sender_has_already_gone() {
    // A secondary launch writes and exits at once, so the owner often
    // accepts a connection whose other end is already closed.
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    sender
        .write_all(&launch("late").to_json().unwrap())
        .unwrap();
    drop(sender);
    assert_eq!(receive(receiver).unwrap(), launch("late"));
}
