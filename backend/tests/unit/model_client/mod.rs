mod capture;
mod endpoint_paths;
mod first_token;
mod generation;
mod metadata;
mod response_reading;

use super::{
    test_support::{check, request, target, Response, Server},
    *,
};
use serde_json::json;
use std::time::Duration;

mod catalog_requests;
mod check_execution;
mod request_headers;
mod request_parameters;
mod sse_decoding;
