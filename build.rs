// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

//! This build script downloads some diagrams from Cacoo and places them into
//! the docs directory, so they can be linked or embedded in the docs.
//!
//! Currently the diagrams are only downloaded when cargo is run with
//! the `generate-diagrams` feature enabled.

#![allow(unused)]

#[cfg(feature = "generate-diagrams")]
extern crate hyper;

#[cfg(feature = "generate-diagrams")]
use hyper::Client;
#[cfg(feature = "generate-diagrams")]
use hyper::client::IntoUrl;
#[cfg(feature = "generate-diagrams")]
use std::env;
#[cfg(feature = "generate-diagrams")]
use std::fs::{self, File};
#[cfg(feature = "generate-diagrams")]
use std::io;
#[cfg(feature = "generate-diagrams")]
use std::path::{Path, PathBuf};

// Only generate the diagrams when "generate-diagrams" feature is enabled.
// TODO: instead of this feature, detect that cargo is run in the "doc" profile.
#[cfg(feature = "generate-diagrams")]
fn main() {
    // List all diagrams names and URLs to download them from.
    download_image("bootstrap",
                   "https://cacoo.com/diagrams/cqX6QPN90ZuKXZ0n-F56A2.png");
    download_image("get-close-group",
                   "https://cacoo.com/diagrams/PTBt1OgHVcdu0PKt-F56A2.png");
    download_image("new-node",
                   "https://cacoo.com/diagrams/5VCFe286q4yfQ6Pm-F56A2.png");
    download_image("tunnel",
                   "https://cacoo.com/diagrams/ALXyW2ugR92IxgWd-F56A2.png");
}

#[cfg(feature = "generate-diagrams")]
fn download_image<U: IntoUrl>(name: &str, src: U) {
    download(src, image_path(name))
}

#[cfg(feature = "generate-diagrams")]
fn download<U: IntoUrl, P: AsRef<Path>>(src: U, dst: P) {
    let client = Client::new();
    let mut res = client.get(src).send().unwrap();

    if let Some(dir) = dst.as_ref().parent() {
        fs::create_dir_all(dir).unwrap();
    }

    let mut file = File::create(dst).unwrap();

    io::copy(&mut res, &mut file).unwrap();
}

#[cfg(feature = "generate-diagrams")]
fn image_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from("target/doc/routing");
    path.push(name);
    path.set_extension("png");
    path
}

#[cfg(not(feature = "generate-diagrams"))]
fn main() {}
