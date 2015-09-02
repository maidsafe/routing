// Copyright 2015 MaidSafe.net limited.
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

use std::io;
use std::convert::From;
use cbor::CborError;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use std::error;
use std::fmt;
use std::str;
use data::Data;

//------------------------------------------------------------------------------
#[deny(missing_docs)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, RustcEncodable, RustcDecodable)]
/// represents response errors
pub enum ResponseError {
    /// Abort is for user to indicate that the state can be dropped;
    /// if received by routing, it will drop the state.
    Abort,
    /// invalid request
    InvalidRequest(Data),
    /// failure to complete request for data
    FailedRequestForData(Data),
    /// had to clear Sacrificial Data in order to complete request
    HadToClearSacrificial(::NameType, u32),
}

impl From<CborError> for ResponseError {
    fn from(e: CborError) -> ResponseError {
        ResponseError::Abort
    }
}

impl error::Error for ResponseError {
    fn description(&self) -> &str {
        match *self {
            ResponseError::Abort => "Abort",
            ResponseError::InvalidRequest(_) => "Invalid request",
            ResponseError::FailedRequestForData(_) => "Failed request for data",
            ResponseError::HadToClearSacrificial(_, _) => "Had to clear Sacrificial data to
              complete request",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ResponseError::Abort => fmt::Display::fmt("ResponseError:: Abort", f),
            ResponseError::InvalidRequest(_) => fmt::Display::fmt("ResponsError::InvalidRequest",
                                  f),
            ResponseError::FailedRequestForData(_) =>
                fmt::Display::fmt("ResponseError::FailedToStoreData", f),
            ResponseError::HadToClearSacrificial(_, _) =>
                fmt::Display::fmt("ResponseError::HadToClearSacrificial", f),
        }
    }
}


//------------------------------------------------------------------------------
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum InterfaceError {
    NotConnected,
}

impl error::Error for InterfaceError {
    fn description(&self) -> &str {
        match *self {
            InterfaceError::NotConnected => "Not Connected",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            _ => None,
        }
    }
}

impl fmt::Display for InterfaceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InterfaceError::NotConnected => fmt::Display::fmt("InterfaceError::NotConnected", f),
        }
    }
}

//------------------------------------------------------------------------------
pub enum ClientError {
    Io(io::Error),
    Cbor(CborError),
}

impl From<CborError> for ClientError {
    fn from(e: CborError) -> ClientError {
        ClientError::Cbor(e)
    }
}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> ClientError {
        ClientError::Io(e)
    }
}

//------------------------------------------------------------------------------
#[deny(missing_docs)]
#[derive(Debug)]
/// Represents routing error types
pub enum RoutingError {
    /// The node/client has not bootstrapped yet
    NotBootstrapped,
    /// invalid requester or handler authorities
    BadAuthority,
    /// failure to connect to an already connected node
    AlreadyConnected,
    /// received message having unknown type
    UnknownMessageType,
    /// Failed signature check
    FailedSignature,
    /// Not Enough signatures
    NotEnoughSignatures,
    /// Duplicate signatures
    DuplicateSignatures,
    /// duplicate request received
    FilterCheckFailed,
    /// failure to bootstrap off the provided endpoints
    FailedToBootstrap,
    /// unexpected empty routing table
    RoutingTableEmpty,
    /// public id rejected because of unallowed relocated status
    RejectedPublicId,
    /// routing table did not add the node information,
    /// either because it was already added, or because it did not improve the routing table
    RefusedFromRoutingTable,
    /// We received a refresh message but it did not contain group source address
    RefreshNotFromGroup,
    /// String errors
    Utf8(str::Utf8Error),
    /// interface error
    Interface(InterfaceError),
    /// i/o error
    Io(io::Error),
    /// serialisation error
    Cbor(CborError),
    /// invalid response
    Response(ResponseError),
    /// crust error
    Crust(::crust::error::Error),
}

impl From<str::Utf8Error> for RoutingError {
    fn from(e: str::Utf8Error) -> RoutingError {
        RoutingError::Utf8(e)
    }
}


impl From<ResponseError> for RoutingError {
    fn from(e: ResponseError) -> RoutingError {
        RoutingError::Response(e)
    }
}

impl From<CborError> for RoutingError {
    fn from(e: CborError) -> RoutingError {
        RoutingError::Cbor(e)
    }
}

impl From<io::Error> for RoutingError {
    fn from(e: io::Error) -> RoutingError {
        RoutingError::Io(e)
    }
}

impl From<InterfaceError> for RoutingError {
    fn from(e: InterfaceError) -> RoutingError {
        RoutingError::Interface(e)
    }
}

impl From<::crust::error::Error> for RoutingError {
    fn from(e: ::crust::error::Error) -> RoutingError {
        RoutingError::Crust(e)
    }
}

impl error::Error for RoutingError {
    fn description(&self) -> &str {
        match *self {
            RoutingError::NotBootstrapped => "Not bootstrapped",
            RoutingError::BadAuthority => "Invalid authority",
            RoutingError::AlreadyConnected => "Already connected",
            RoutingError::UnknownMessageType => "Invalid message type",
            RoutingError::FilterCheckFailed => "Filter check failure",
            RoutingError::FailedSignature => "Signature check failure",
            RoutingError::NotEnoughSignatures => "Not enough signatures",
            RoutingError::DuplicateSignatures => "Not enough signatures",
            RoutingError::FailedToBootstrap => "Could not bootstrap",
            RoutingError::RoutingTableEmpty => "Routing table empty",
            RoutingError::RejectedPublicId => "Rejected Public Id",
            RoutingError::RefusedFromRoutingTable => "Refused from routing table",
            RoutingError::RefreshNotFromGroup => "Refresh message not from group",
            RoutingError::Utf8(_) => "String/Utf8 error",
            RoutingError::Interface(_) => "Interface error",
            RoutingError::Io(_) => "I/O error",
            RoutingError::Cbor(_) => "Serialisation error",
            RoutingError::Response(_) => "Response error",
            RoutingError::Crust(_) => "Crust error",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            RoutingError::Interface(ref err) => Some(err as &error::Error),
            RoutingError::Io(ref err) => Some(err as &error::Error),
            // RoutingError::Cbor(ref err) => Some(err as &error::Error),
            RoutingError::Response(ref err) => Some(err as &error::Error),
            _ => None,
        }
    }
}

impl fmt::Display for RoutingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RoutingError::NotBootstrapped => fmt::Display::fmt("Not bootstrapped", f),
            RoutingError::BadAuthority => fmt::Display::fmt("Bad authority", f),
            RoutingError::AlreadyConnected => fmt::Display::fmt("already connected", f),
            RoutingError::UnknownMessageType => fmt::Display::fmt("Unknown message", f),
            RoutingError::FilterCheckFailed => fmt::Display::fmt("filter check failed", f),
            RoutingError::FailedSignature => fmt::Display::fmt("Signature check failed", f),
            RoutingError::NotEnoughSignatures => fmt::Display::fmt("Not enough signatures \
                                   (multi-sig)", f),
            RoutingError::DuplicateSignatures => fmt::Display::fmt("Duplicated signatures \
                                   (multi-sig)", f),
            RoutingError::FailedToBootstrap => fmt::Display::fmt("could not bootstrap", f),
            RoutingError::RoutingTableEmpty => fmt::Display::fmt("routing table empty", f),
            RoutingError::RejectedPublicId => fmt::Display::fmt("Rejected Public Id", f),
            RoutingError::RefusedFromRoutingTable =>
                fmt::Display::fmt("Refused from routing table", f),
            RoutingError::RefreshNotFromGroup =>
                fmt::Display::fmt("Refresh message not from group", f),
            RoutingError::Utf8(ref err) => fmt::Display::fmt(err, f),
            RoutingError::Interface(ref err) => fmt::Display::fmt(err, f),
            RoutingError::Io(ref err) => fmt::Display::fmt(err, f),
            RoutingError::Cbor(ref err) => fmt::Display::fmt(err, f),
            RoutingError::Response(ref err) => fmt::Display::fmt(err, f),
            RoutingError::Crust(ref err) => fmt::Display::fmt(err, f),
        }
    }
}

#[cfg(test)]
mod test {
    //FIXME (ben 18/08/2015) Tests can be expanded
    use super::*;
    use rustc_serialize::{Decodable, Encodable};
    use cbor;

    fn test_object<T>(obj_before: T)
        where T: for<'a> Encodable + Decodable + Eq
    {
        let mut e = cbor::Encoder::from_memory();
        e.encode(&[&obj_before]).unwrap();
        let mut d = cbor::Decoder::from_bytes(e.as_bytes());
        let obj_after: T = d.decode().next().unwrap().unwrap();
        assert_eq!(obj_after == obj_before, true)
    }

    #[test]
    fn test_response_error() {
        test_object(ResponseError::Abort)
    }
}
