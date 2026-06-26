use crate::async_rt::run_until_stalled;
use crate::reactive::create_root;

use super::{get, post, set_client, MockClient, MultipartForm, Response};

#[test]
fn multipart_serializes_fields_and_files() {
    let form = MultipartForm::new()
        .field("title", "Receipt")
        .file("doc", "a.txt", "text/plain", b"hello".to_vec());
    assert_eq!(form.len(), 2);

    let (content_type, body) = form.build();
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    let boundary = content_type.split("boundary=").nth(1).unwrap();

    let text = String::from_utf8(body).unwrap();
    // Each part is introduced by the boundary marker.
    assert_eq!(text.matches(&format!("--{boundary}\r\n")).count(), 2);
    assert!(text.contains("Content-Disposition: form-data; name=\"title\"\r\n\r\nReceipt\r\n"));
    assert!(text.contains(
        "Content-Disposition: form-data; name=\"doc\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nhello\r\n"
    ));
    // Closing boundary.
    assert!(text.ends_with(&format!("--{boundary}--\r\n")));
}

#[test]
fn multipart_boundaries_are_unique() {
    let (ct1, _) = MultipartForm::new().field("a", "1").build();
    let (ct2, _) = MultipartForm::new().field("a", "1").build();
    assert_ne!(ct1, ct2, "each build must mint a fresh boundary");
}

#[test]
fn get_resolves_with_mock_response() {
    set_client(MockClient::new(|req| {
        assert_eq!(req.url, "https://api.test/ping");
        Ok(Response::ok("pong"))
    }));

    let (res, scope) = create_root(|| get("https://api.test/ping"));
    assert!(res.loading());
    run_until_stalled();
    let r = res.data().expect("resolved");
    assert!(r.is_success());
    assert_eq!(r.body, "pong");
    scope.dispose();
}

#[test]
fn post_sends_body_and_errors_propagate() {
    set_client(MockClient::new(|req| {
        if req.body.as_deref() == Some("hi") {
            Ok(Response::ok("got it"))
        } else {
            Err("bad body".to_string())
        }
    }));

    let (ok, scope) = create_root(|| post("https://api.test/echo", "hi"));
    run_until_stalled();
    assert_eq!(ok.data().unwrap().body, "got it");
    scope.dispose();

    let (bad, scope2) = create_root(|| post("https://api.test/echo", "nope"));
    run_until_stalled();
    assert_eq!(bad.error().as_deref(), Some("bad body"));
    scope2.dispose();
}

#[test]
fn unconfigured_client_reports_error() {
    // A fresh thread: no client set -> the default reports a clear error.
    std::thread::spawn(|| {
        let (res, scope) = create_root(|| get("x"));
        run_until_stalled();
        assert!(res.error().unwrap().contains("no HTTP client"));
        scope.dispose();
    })
    .join()
    .unwrap();
}
