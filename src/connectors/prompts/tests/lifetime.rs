use super::*;
use crate::git::process::Operation;
use std::os::unix::fs::PermissionsExt;
#[test]
fn operation_close_settles_idle_prompt_wait_and_cleans_socket() {
    let temp = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(temp.path(), None).unwrap();
    let socket = bridge.socket();
    assert_eq!(
        fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut command = std::process::Command::new("/bin/sleep");
    command.arg("60");
    let operation = Operation::spawn(command, vec![]);
    let cancel = operation.cancellation();
    let waiter = std::thread::spawn(move || {
        bridge.serve_one(&cancel, Duration::from_secs(60), |_| panic!())
    });
    drop(operation);
    assert!(waiter.join().unwrap().is_err());
    assert!(!socket.exists());
}
#[test]
fn sequence_editor_abort_and_wrong_file_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("todo");
    fs::write(&file, "pick fixture\n").unwrap();
    let mut bridge = Bridge::new(temp.path(), Some(file.clone())).unwrap();
    let socket = bridge.socket();
    let token = bridge.helper_token().to_owned();
    let helper_file = file.clone();
    let helper = std::thread::spawn(move || {
        request(
            &socket,
            &token,
            1,
            Request::SequenceEditor { path: helper_file },
        )
    });
    bridge
        .serve_one(&AtomicBool::new(false), Duration::from_secs(2), |_| {
            Reply::Abort
        })
        .unwrap();
    assert!(matches!(helper.join().unwrap().unwrap(), Reply::Abort));
    assert_eq!(fs::read_to_string(&file).unwrap(), "pick fixture\n");
    let socket = bridge.socket();
    let token = bridge.helper_token().to_owned();
    let helper = std::thread::spawn(move || {
        request(
            &socket,
            &token,
            2,
            Request::Editor {
                path: "/not-the-operation-file".into(),
            },
        )
    });
    assert!(
        bridge
            .serve_one(
                &AtomicBool::new(false),
                Duration::from_secs(2),
                |_| panic!()
            )
            .is_err()
    );
    assert!(helper.join().unwrap().is_err());
}
