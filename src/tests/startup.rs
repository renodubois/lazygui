use super::*;
#[test]
fn byte_paths_inline_values_and_usage_failures() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let args = vec![
        OsString::from_vec(b"--path=repo-\xff".to_vec()),
        "--use-config-file".into(),
        OsString::from_vec(b"config-\xfe.yml".to_vec()),
    ];
    let parsed = arguments(args, "/fixture".into()).unwrap();
    assert_eq!(parsed.cwd.as_os_str().as_bytes(), b"/fixture/repo-\xff");
    assert_eq!(
        parsed.global_source_cwd.as_deref(),
        Some(std::path::Path::new("/fixture"))
    );
    assert_eq!(
        parsed.cli_config_file.unwrap().as_os_str().as_bytes(),
        b"config-\xfe.yml"
    );
    assert!(arguments(vec!["--path".into()], "/fixture".into()).is_err());
    assert!(arguments(vec!["--unknown".into()], "/fixture".into()).is_err());
    assert!(arguments(vec!["--path=".into()], "/fixture".into()).is_err());
}
