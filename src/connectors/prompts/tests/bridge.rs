use super::*;
#[test]
fn authenticated_fake_askpass_editor_replay_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("message");
    fs::write(&file, "old").unwrap();
    let mut bridge = Bridge::new(temp.path(), Some(file.clone())).unwrap();
    let socket = bridge.socket();
    let token = bridge.helper_token().to_owned();
    let helper_socket = socket.clone();
    let helper_token = token.clone();
    let helper = std::thread::spawn(move || {
        request(
            &helper_socket,
            &helper_token,
            1,
            Request::Askpass {
                label: "fake password".into(),
            },
        )
        .unwrap()
    });
    bridge
        .serve_one(&AtomicBool::new(false), Duration::from_secs(2), |_| {
            Reply::Value("disposable".into())
        })
        .unwrap();
    assert!(matches!(helper.join().unwrap(), Reply::Value(value) if value == "disposable"));
    let helper_socket = socket.clone();
    let helper_file = file.clone();
    let helper_token = token.clone();
    let helper = std::thread::spawn(move || {
        request(
            &helper_socket,
            &helper_token,
            2,
            Request::Editor { path: helper_file },
        )
        .unwrap()
    });
    bridge
        .serve_one(&AtomicBool::new(false), Duration::from_secs(2), |request| {
            let Request::Editor { path } = request else {
                panic!()
            };
            fs::write(path, "native edit").unwrap();
            Reply::Edited
        })
        .unwrap();
    assert!(matches!(helper.join().unwrap(), Reply::Edited));
    assert_eq!(fs::read_to_string(file).unwrap(), "native edit");
    let helper_socket = socket.clone();
    let helper = std::thread::spawn(move || {
        request(
            &helper_socket,
            &token,
            2,
            Request::Askpass {
                label: "replay".into(),
            },
        )
    });
    assert!(
        bridge
            .serve_one(&AtomicBool::new(false), Duration::from_secs(2), |_| panic!(
                "replay must not dispatch"
            ))
            .is_err()
    );
    assert!(helper.join().unwrap().is_err());
    drop(bridge);
    assert!(!socket.exists());
}
#[test]
fn forged_bounded_and_cancelled_waits_never_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(temp.path(), None).unwrap();
    let socket = bridge.socket();
    let helper = std::thread::spawn(move || {
        request(
            &socket,
            "forged",
            1,
            Request::Askpass {
                label: "fake".into(),
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
    let mut stream = UnixStream::connect(bridge.socket()).unwrap();
    stream.write_all(&((MAX + 1) as u32).to_be_bytes()).unwrap();
    assert!(
        bridge
            .serve_one(
                &AtomicBool::new(false),
                Duration::from_secs(2),
                |_| panic!()
            )
            .is_err()
    );
    assert!(
        bridge
            .serve_one(&AtomicBool::new(true), Duration::from_secs(2), |_| panic!())
            .is_err()
    );
    assert!(
        bridge
            .serve_one(
                &AtomicBool::new(false),
                Duration::from_millis(10),
                |_| panic!()
            )
            .is_err()
    );
}
