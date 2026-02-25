#![allow(unused)]

use super::*;
use picoserve::response::{IntoResponse};

#[cfg(feature = "std-mode")]
use std::println;

#[derive(serde::Deserialize)]
pub struct NewQuery {
    name: String,
    size: usize
}

#[derive(serde::Deserialize)]
struct ChunkQuery {
    name: String,
    idx: usize
}

pub async fn new(query: picoserve::extract::Query<NewQuery>) -> impl IntoResponse {
    println!("name = {}, size = {}", query.name, query.size);
    "success"
}

pub async fn chunk() -> impl IntoResponse {
    "success"
}

pub async fn end() -> impl IntoResponse {
    "success"
}
