# routing

**Primary Maintainer:**     Benjamin Bollen (benjamin.bollen@maidsafe.net)

**Secondary Maintainer:**   Peter Jankuliak (peter.jankuliak@maidsafe.net)

Routing - a specialised storage DHT

|Crate|Linux|Windows|OSX|Coverage|
|:------:|:-------:|:-------:|:-------:|:-------:|
|[![](http://meritbadge.herokuapp.com/routing)](https://crates.io/crates/routing)|[![Build Status](https://travis-ci.org/maidsafe/routing.svg?branch=master)](https://travis-ci.org/maidsafe/routing)|[![Build Status](http://ci.maidsafe.net:8080/buildStatus/icon?job=routing_win64_status_badge)](http://ci.maidsafe.net:8080/job/routing_win64_status_badge/)|[![Build Status](http://ci.maidsafe.net:8080/buildStatus/icon?job=routing_osx_status_badge)](http://ci.maidsafe.net:8080/job/routing_osx_status_badge/)|[![Coverage Status](https://coveralls.io/repos/maidsafe/routing/badge.svg)](https://coveralls.io/r/maidsafe/routing)|

| [API Documentation](http://maidsafe.github.io/routing/routing/) | [SAFENetwork System Documention](http://systemdocs.maidsafe.net/) | [MaidSafe website](http://www.maidsafe.net) | [Safe Community site](https://forum.safenetwork.io) |

#Overview

A secured [DHT](http://en.wikipedia.org/wiki/Distributed_hash_table), based on a [kademlia-like](http://en.wikipedia.org/wiki/Kademlia) implementation, but with some very stark differences. This is a recursive as opposed to iterative network, enabling easier NAT traversal and providing more efficient use of routers and larger networks. This also allows very fast reconfiguration of network changes, aleviating the requirement for a refresh algorithm. A recursive solution based on a network protocol layer that is 'connection oriented' also allows a close group to be aligned with security protocols.

This library makes use of [Public-key cryptography](http://en.wikipedia.org/wiki/Public-key_cryptography) to allow a mechanism to ensure nodes are well recognised and cryptographically secured. This pattern allows the creation of a DHT based PKI and this in turn allows a decentralised network to make use of groups as fixed in relation to any address. This is particularly useful in a continually fluid network as described [here,](http://maidsafe.net/Whitepapers/pdf/MaidSafeDistributedHashTable.pdf) creating a server-less and [autonomous network](http://maidsafe.net/docs/SAFEnetwork.pdf).

This is a very under researched area. For a general introduction to some of the ideas behind the design related to XOR Space, watching [The SAFE Network from First Principles series](https://www.youtube.com/watch?v=Lr9FJRDcNzk&list=PLiYqQVdgdw_sSDkdIZzDRQR9xZlsukIxD) is recommended. The slides for XOR Distance Metric and Basic Routing lecture are also [available here](http://ericklavoie.com/talks/safenetwork/1-xor-routing.pdf). The last video from the series on how the same ideas were applied to decentralised BitTorrent trackers is available [here](https://www.youtube.com/watch?v=YFV908uoLPY). A proper formalisation of the Routing algorithm is in progress.


###Pre-requisite:
libsodium is a native dependency for [sodiumxoide](https://github.com/dnaq/sodiumoxide). Thus, install sodium by following the instructions [here](http://doc.libsodium.org/installation/index.html).

For windows, download and use the [prebuilt mingw library](https://download.libsodium.org/libsodium/releases/libsodium-1.0.2-mingw.tar.gz).
Extract and place the libsodium.a file in "bin\x86_64-pc-windows-gnu" for 64bit System, or "bin\i686-pc-windows-gnu" for a 32bit system.

##Todo Items

General note: please document code you touch, and introduce property-based unit tests where applicable.

## [0.1.60] - essential logical corrections
- [x] [MAID-1007](https://maidsafe.atlassian.net/browse/MAID-1007) limit swarm to targeted group
 - [x] [MAID-1105](https://maidsafe.atlassian.net/browse/MAID-1105) delay RoutingTable new ConnectRequests
 - [x] [MAID-1106](https://maidsafe.atlassian.net/browse/MAID-1106) examine Not For Us
- [x] [MAID-1032](https://maidsafe.atlassian.net/browse/MAID-1032)
correct name calculation of pure Id
- [x] [MAID-1034](https://maidsafe.atlassian.net/browse/MAID-1034) ConnectResponse needs to include original signed ConnectRequest
- [x] [MAID-1043](https://maidsafe.atlassian.net/browse/MAID-1043) remove old sentinel
- [x] [MAID-1059](https://maidsafe.atlassian.net/browse/MAID-1059) rename types::Action -> types::MessageAction; rename RoutingNodeAction -> MethodCall

## [0.1.61] - Relay module, relocatable Id, update NodeInterface

- [x] [MAID-1114](https://maidsafe.atlassian.net/browse/MAID-1114) Relay module
- [x] [MAID-1060](https://maidsafe.atlassian.net/browse/MAID-1060) update Interface for Vaults
- [x] [MAID-1040](https://maidsafe.atlassian.net/browse/MAID-1040) enable Id, PublicId and NodeInfo with 'relocated' name

## [0.1.62] - restructure core of routing

- [x] Update dependecy to CRUSTv0.0.60 : resolved loopback endpoint problem
- [ ] [MAID-1037](https://maidsafe.atlassian.net/browse/MAID-1037) Address relocation
  - [ ] [MAID-1038](https://maidsafe.atlassian.net/browse/MAID-1038) Integrate handlers with RelayMap
  - [x] [MAID-1039](https://maidsafe.atlassian.net/browse/MAID-1039) put_public_id handler
- [ ] [MAID-1052](https://maidsafe.atlassian.net/browse/MAID-1052) Message Handling
  - [ ] [MAID-1055](https://maidsafe.atlassian.net/browse/MAID-1055) full review of implementation of handlers
  - [x] [MAID-1057](https://maidsafe.atlassian.net/browse/MAID-1057) make event loop in routing_node internal
- [ ] [MAID-1062](https://maidsafe.atlassian.net/browse/MAID-1062) extract all_connections into a module
- [ ] [MAID-1070](https://maidsafe.atlassian.net/browse/MAID-1070) drop_bootstrap in coordination with CRUST
- [ ] [MAID-1071](https://maidsafe.atlassian.net/browse/MAID-1071) Implement relay id exchange for client node
- [ ] [MAID-1066](https://maidsafe.atlassian.net/browse/MAID-1066) Routing Example : update to internal event loop

## [0.1.70] - activate Sentinel and refactor MessageTypeTag

- [ ] [MAID-1042](https://maidsafe.atlassian.net/browse/MAID-1042) Sentinel
    - [ ] [MAID-1045](https://maidsafe.atlassian.net/browse/MAID-1045) plug in Sentinel [Reference document](https://docs.google.com/document/d/1-x7pCq_YXm-P5xDi7y8UIYDbheVwJ10Q80FzgtnMD8A/edit?usp=sharing)
    - [ ] [MAID-1046](https://maidsafe.atlassian.net/browse/MAID-1046) break down (header, body) into correct (request, claim) and dispatch
    - [ ] [MAID-1048](https://maidsafe.atlassian.net/browse/MAID-1048) Ensure correct dispatch of GetGroupKeyResponse
    - [ ] [MAID-1049](https://maidsafe.atlassian.net/browse/MAID-1049) update signature of handler functions to request and claim
    - [ ] [MAID-1050](https://maidsafe.atlassian.net/browse/MAID-1050) block messages at filter once Sentinel has resolved
    - [ ] [MAID-1051](https://maidsafe.atlassian.net/browse/MAID-1051) update construction of message_header
- [ ] [MAID-1063](https://maidsafe.atlassian.net/browse/MAID-1063) replace MessageTypeTag with full enum.
    - [ ] [MAID-1064](https://maidsafe.atlassian.net/browse/MAID-1064) POC first and move UnauthorisedPut into explicit message structure.
- [ ] [MAID-1065](https://maidsafe.atlassian.net/browse/MAID-1065) Return Result for Put Get Post

## [0.1.80] - activate account transfer

- [ ] [MAID-1067](https://maidsafe.atlassian.net/browse/MAID-1067) Remove merge from Sendable
- [ ] [MAID-1068](https://maidsafe.atlassian.net/browse/MAID-1068) Account Transfer message
- [ ] [MAID-1069](https://maidsafe.atlassian.net/browse/MAID-1069) OurCloseGroup Authority

## [0.1.90] Finish sprint

- [ ] [MAID-1072](https://maidsafe.atlassian.net/browse/MAID-1072) Integration test
- [ ] [MAID-1076](https://maidsafe.atlassian.net/browse/MAID-1076) implement Network Visualiser
