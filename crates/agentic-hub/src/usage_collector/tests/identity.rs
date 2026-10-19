use std::collections::HashMap;

use super::super::{
    identity_from_headers, AGENT_REF_HEADER, AGENT_VERSION_HEADER, RUN_ID_HEADER, TASK_ID_HEADER,
};

#[test]
fn absent_identity_headers_are_not_an_error() {
    let identity = identity_from_headers(&HashMap::new()).expect("absent");
    assert!(identity.run_id.is_none());
    assert!(identity.agent_version.is_none());
}

#[test]
fn headers_accept_a_run_and_lowercase_the_version() {
    let mut headers = HashMap::new();
    headers.insert(RUN_ID_HEADER.to_string(), "cmrun123".to_string());
    headers.insert(TASK_ID_HEADER.to_string(), "cmtask123".to_string());
    headers.insert(AGENT_REF_HEADER.to_string(), "cto".to_string());
    headers.insert(AGENT_VERSION_HEADER.to_string(), "0123456789AB".to_string());

    let identity = identity_from_headers(&headers).expect("valid");

    assert_eq!(identity.run_id.as_deref(), Some("cmrun123"));
    assert_eq!(identity.task_id.as_deref(), Some("cmtask123"));
    assert_eq!(identity.agent_ref.as_deref(), Some("cto"));
    assert_eq!(identity.agent_version.as_deref(), Some("0123456789ab"));
}

#[test]
fn headers_reject_a_run_id_with_a_space() {
    let mut headers = HashMap::new();
    headers.insert(RUN_ID_HEADER.to_string(), "cm run".to_string());
    assert!(identity_from_headers(&headers).is_err());
}
