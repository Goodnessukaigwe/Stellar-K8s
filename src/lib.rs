//! Stellar-K8s: Cloud-Native Kubernetes Operator for Stellar Infrastructure
//!
//! This crate provides a Kubernetes operator for managing Stellar Core,
//! Horizon, and Soroban RPC nodes on Kubernetes clusters.

pub mod controller;
pub mod crd;
pub mod error;
pub mod telemetry;

#[cfg(feature = "rest-api")]
pub mod rest_api;

pub use crate::error::{Error, Result};
