var searchIndex = JSON.parse('{\
"sn_routing":{"doc":"Peer implementation for a resilient decentralised network…","i":[[3,"TransportConfig","sn_routing","QuicP2p configurations",null,null],[12,"hard_coded_contacts","","Hard Coded contacts",0,null],[12,"port","","Port we want to reserve for QUIC. If none supplied we\'ll…",0,null],[12,"ip","","IP address for the listener. If none supplied we\'ll use…",0,null],[12,"max_msg_size_allowed","","This is the maximum message size we\'ll allow the peer to…",0,null],[12,"idle_timeout_msec","","If we hear nothing from the peer in the given interval we…",0,null],[12,"keep_alive_interval_msec","","Interval to send keep-alives if we are idling so that the…",0,null],[12,"bootstrap_cache_dir","","Directory in which the bootstrap cache will be stored. If…",0,null],[12,"upnp_lease_duration","","Duration of a UPnP port mapping.",0,null],[3,"Prefix","","A section prefix, i.e. a sequence of bits specifying the…",null,null],[3,"XorName","","A 256-bit number, viewed as a point in XOR space.",null,null],[12,"0","","",1,null],[17,"XOR_NAME_LEN","","Constant byte length of `XorName`.",null,null],[3,"NetworkParams","","Network parameters: number of elders, recommended section…",null,null],[12,"elder_size","","The number of elders per section",2,null],[12,"recommended_section_size","","Recommended number of nodes in a section.",2,null],[3,"Config","","Routing configuration.",null,null],[12,"first","","If true, configures the node to start a new network…",3,null],[12,"keypair","","The `Keypair` of the node or `None` for randomly generated…",3,null],[12,"transport_config","","Configuration for the underlying network transport.",3,null],[12,"network_params","","Global network parameters. Must be identical for all nodes…",3,null],[3,"EventStream","","Stream of routing node events",null,null],[3,"Routing","","Interface for sending and receiving messages to and from…",null,null],[3,"SectionProofChain","","Chain of section BLS keys where every key is proven…",null,null],[4,"Error","","Internal error.",null,null],[13,"BadLocation","","",4,null],[13,"FailedSignature","","",4,null],[13,"CannotRoute","","",4,null],[13,"Network","","",4,null],[13,"InvalidState","","",4,null],[13,"Bincode","","",4,null],[13,"InvalidSource","","",4,null],[13,"InvalidDestination","","",4,null],[13,"InvalidMessage","","",4,null],[13,"UntrustedMessage","","",4,null],[13,"InvalidSignatureShare","","",4,null],[13,"InvalidElderDkgResult","","",4,null],[13,"FailedSend","","",4,null],[13,"InvalidVote","","",4,null],[4,"DstLocation","","Message destination location.",null,null],[13,"Node","","Destination is a single node with the given name.",5,null],[13,"Section","","Destination are the nodes of the section whose prefix…",5,null],[13,"Direct","","Destination is the node at the `ConnectionInfo` the…",5,null],[4,"SrcLocation","","Message source location.",null,null],[13,"Node","","A single node with the given name.",6,null],[13,"Section","","A section with the given prefix.",6,null],[0,"event","","sn_routing events.",null,null],[3,"RecvStream","sn_routing::event","Stream to receive multiple messages",null,null],[3,"SendStream","","Stream of outgoing messages",null,null],[4,"Connected","","An Event raised as node complete joining",null,null],[13,"First","","Node first joining the network",7,null],[13,"Relocate","","Node relocating from one section to another",7,null],[12,"previous_name","sn_routing::event::Connected","Previous name before relocation.",8,null],[4,"Event","sn_routing::event","An Event raised by a `Node` or `Client` via its event…",null,null],[13,"Connected","","The node has successfully connected to the network.",9,null],[13,"MessageReceived","","Received a message.",9,null],[12,"content","sn_routing::event::Event","The content of the message.",10,null],[12,"src","","The source location that sent the message.",10,null],[12,"dst","","The destination location that receives the message.",10,null],[13,"PromotedToElder","sn_routing::event","The node has been promoted to elder",9,null],[13,"PromotedToAdult","","The node has been promoted to adult",9,null],[13,"Demoted","","The node has been demoted from elder",9,null],[13,"MemberJoined","","An adult or elder joined our section.",9,null],[12,"name","sn_routing::event::Event","Name of the node",11,null],[12,"previous_name","","Previous name before relocation",11,null],[12,"age","","Age of the node",11,null],[13,"InfantJoined","sn_routing::event","An infant node joined our section.",9,null],[12,"name","sn_routing::event::Event","Name of the node",12,null],[12,"age","","Age of the node",12,null],[13,"MemberLeft","sn_routing::event","A node left our section.",9,null],[12,"name","sn_routing::event::Event","Name of the node",13,null],[12,"age","","Age of the node",13,null],[13,"EldersChanged","sn_routing::event","The set of elders in our section has changed.",9,null],[12,"prefix","sn_routing::event::Event","The prefix of our section.",14,null],[12,"key","","The BLS public key of our section.",14,null],[12,"elders","","The set of elders of our section.",14,null],[13,"RelocationStarted","sn_routing::event","This node has started relocating to other section. Will be…",9,null],[12,"previous_name","sn_routing::event::Event","Previous name before relocation",15,null],[13,"RestartRequired","sn_routing::event","Disconnected or failed to connect - restart required.",9,null],[13,"ClientMessageReceived","","Received a message from a client node.",9,null],[12,"content","sn_routing::event::Event","The content of the message.",16,null],[12,"src","","The address of the client that sent the message.",16,null],[12,"send","","Stream to send messages back to the client that sent the…",16,null],[12,"recv","","Stream to receive more messages from the client on the…",16,null],[0,"log_ident","sn_routing","Log identifier - a short string that is prefixed in front…",null,null],[5,"set","sn_routing::log_ident","Set the log identifier for the current task.",null,[[["string",3]]]],[5,"get","","Get the current log identifier.",null,[[],[["string",3],["arc",3]]]],[0,"rng","sn_routing","Random number generation Random number generation utilities.",null,null],[3,"MainRng","sn_routing::rng","A random number generator that retrieves randomness from…",null,null],[5,"new","","Create new rng instance.",null,[[],["mainrng",3]]],[11,"is_section","sn_routing","Returns whether this location is a section.",6,[[]]],[11,"to_dst","","Returns this location as `DstLocation`",6,[[],["dstlocation",4]]],[11,"is_section","","Returns whether this location is a section.",5,[[]]],[11,"next","","Returns next event",17,[[]]],[11,"new","","Create new node using the given config.",18,[[["config",3]]]],[11,"public_key","","Returns the `PublicKey` of this node.",18,[[]]],[11,"sign","","Sign any data with the key of this node.",18,[[]]],[11,"verify","","Verify any signed data with the key of this node.",18,[[["signature",3]]]],[11,"name","","The name of this node.",18,[[]]],[11,"our_connection_info","","Returns connection info of this node.",18,[[],[["result",6],["socketaddr",4]]]],[11,"our_prefix","","Our `Prefix` once we are a part of the section.",18,[[]]],[11,"matches_our_prefix","","Finds out if the given XorName matches our prefix. Returns…",18,[[["xorname",3]]]],[11,"is_elder","","Returns whether the node is Elder.",18,[[]]],[11,"our_elders","","Returns the information of all the current section elders.",18,[[]]],[11,"our_elders_sorted_by_distance_to","","Returns the elders of our section sorted by their distance…",18,[[["xorname",3]]]],[11,"our_adults","","Returns the information of all the current section adults.",18,[[]]],[11,"our_adults_sorted_by_distance_to","","Returns the adults of our section sorted by their distance…",18,[[["xorname",3]]]],[11,"our_section","","Returns the info about our section or `None` if we are not…",18,[[]]],[11,"neighbour_sections","","Returns the info about our neighbour sections.",18,[[]]],[11,"send_message","","Send a message.",18,[[["srclocation",4],["dstlocation",4],["bytes",3]]]],[11,"send_message_to_client","","Send a message to a client peer.",18,[[["bytes",3],["socketaddr",4]]]],[11,"public_key_set","","Returns the current BLS public key set or…",18,[[]]],[11,"secret_key_share","","Returns the current BLS secret key share or…",18,[[]]],[11,"our_history","","Returns our section proof chain, or `None` if we are not…",18,[[]]],[11,"our_index","","Returns our index in the current BLS group or…",18,[[]]],[11,"new","","Creates new chain consisting of only one block.",19,[[["publickey",3]]]],[11,"first_key","","Returns the first key of the chain.",19,[[],["publickey",3]]],[11,"last_key","","Returns the last key of the chain.",19,[[],["publickey",3]]],[11,"keys","","Returns all the keys of the chain as a DoubleEndedIterator.",19,[[]]],[11,"has_key","","Returns whether this chain contains the given key.",19,[[["publickey",3]]]],[11,"index_of","","Returns the index of the key in the chain or `None` if not…",19,[[["publickey",3]],["option",4]]],[11,"slice","","Returns a subset of this chain specified by the given…",19,[[["rangebounds",8]]]],[11,"len","","Number of blocks in the chain (including the first block)",19,[[]]],[11,"last_key_index","","Index of the last key in the chain.",19,[[]]],[11,"self_verify","","Check that all the blocks in the chain except the first…",19,[[]]],[11,"check_trust","","Verify this proof chain against the given trusted keys.",19,[[],["truststatus",4]]],[6,"Result","","The type returned by the sn_routing message handling…",null,null],[17,"MIN_AGE","","The minimum age a node can have. The Infants will start at…",null,null],[11,"from","","",0,[[]]],[11,"into","","",0,[[]]],[11,"to_owned","","",0,[[]]],[11,"clone_into","","",0,[[]]],[11,"try_from","","",0,[[],["result",4]]],[11,"try_into","","",0,[[],["result",4]]],[11,"borrow","","",0,[[]]],[11,"borrow_mut","","",0,[[]]],[11,"type_id","","",0,[[],["typeid",3]]],[11,"vzip","","",0,[[]]],[11,"equivalent","","",0,[[]]],[11,"clear","","",0,[[]]],[11,"initialize","","",0,[[]]],[11,"from","","",20,[[]]],[11,"into","","",20,[[]]],[11,"to_owned","","",20,[[]]],[11,"clone_into","","",20,[[]]],[11,"try_from","","",20,[[],["result",4]]],[11,"try_into","","",20,[[],["result",4]]],[11,"borrow","","",20,[[]]],[11,"borrow_mut","","",20,[[]]],[11,"type_id","","",20,[[],["typeid",3]]],[11,"vzip","","",20,[[]]],[11,"equivalent","","",20,[[]]],[11,"clear","","",20,[[]]],[11,"initialize","","",20,[[]]],[11,"from","","",1,[[]]],[11,"into","","",1,[[]]],[11,"to_owned","","",1,[[]]],[11,"clone_into","","",1,[[]]],[11,"to_string","","",1,[[],["string",3]]],[11,"try_from","","",1,[[],["result",4]]],[11,"try_into","","",1,[[],["result",4]]],[11,"borrow","","",1,[[]]],[11,"borrow_mut","","",1,[[]]],[11,"type_id","","",1,[[],["typeid",3]]],[11,"vzip","","",1,[[]]],[11,"equivalent","","",1,[[]]],[11,"clear","","",1,[[]]],[11,"initialize","","",1,[[]]],[11,"from","","",2,[[]]],[11,"into","","",2,[[]]],[11,"to_owned","","",2,[[]]],[11,"clone_into","","",2,[[]]],[11,"try_from","","",2,[[],["result",4]]],[11,"try_into","","",2,[[],["result",4]]],[11,"borrow","","",2,[[]]],[11,"borrow_mut","","",2,[[]]],[11,"type_id","","",2,[[],["typeid",3]]],[11,"vzip","","",2,[[]]],[11,"clear","","",2,[[]]],[11,"initialize","","",2,[[]]],[11,"from","","",3,[[]]],[11,"into","","",3,[[]]],[11,"try_from","","",3,[[],["result",4]]],[11,"try_into","","",3,[[],["result",4]]],[11,"borrow","","",3,[[]]],[11,"borrow_mut","","",3,[[]]],[11,"type_id","","",3,[[],["typeid",3]]],[11,"vzip","","",3,[[]]],[11,"clear","","",3,[[]]],[11,"initialize","","",3,[[]]],[11,"from","","",17,[[]]],[11,"into","","",17,[[]]],[11,"try_from","","",17,[[],["result",4]]],[11,"try_into","","",17,[[],["result",4]]],[11,"borrow","","",17,[[]]],[11,"borrow_mut","","",17,[[]]],[11,"type_id","","",17,[[],["typeid",3]]],[11,"vzip","","",17,[[]]],[11,"from","","",18,[[]]],[11,"into","","",18,[[]]],[11,"try_from","","",18,[[],["result",4]]],[11,"try_into","","",18,[[],["result",4]]],[11,"borrow","","",18,[[]]],[11,"borrow_mut","","",18,[[]]],[11,"type_id","","",18,[[],["typeid",3]]],[11,"vzip","","",18,[[]]],[11,"from","","",19,[[]]],[11,"into","","",19,[[]]],[11,"to_owned","","",19,[[]]],[11,"clone_into","","",19,[[]]],[11,"try_from","","",19,[[],["result",4]]],[11,"try_into","","",19,[[],["result",4]]],[11,"borrow","","",19,[[]]],[11,"borrow_mut","","",19,[[]]],[11,"type_id","","",19,[[],["typeid",3]]],[11,"vzip","","",19,[[]]],[11,"equivalent","","",19,[[]]],[11,"from","","",4,[[]]],[11,"into","","",4,[[]]],[11,"to_string","","",4,[[],["string",3]]],[11,"try_from","","",4,[[],["result",4]]],[11,"try_into","","",4,[[],["result",4]]],[11,"borrow","","",4,[[]]],[11,"borrow_mut","","",4,[[]]],[11,"type_id","","",4,[[],["typeid",3]]],[11,"vzip","","",4,[[]]],[11,"as_fail","","",4,[[],["fail",8]]],[11,"from","","",5,[[]]],[11,"into","","",5,[[]]],[11,"to_owned","","",5,[[]]],[11,"clone_into","","",5,[[]]],[11,"try_from","","",5,[[],["result",4]]],[11,"try_into","","",5,[[],["result",4]]],[11,"borrow","","",5,[[]]],[11,"borrow_mut","","",5,[[]]],[11,"type_id","","",5,[[],["typeid",3]]],[11,"vzip","","",5,[[]]],[11,"equivalent","","",5,[[]]],[11,"from","","",6,[[]]],[11,"into","","",6,[[]]],[11,"to_owned","","",6,[[]]],[11,"clone_into","","",6,[[]]],[11,"try_from","","",6,[[],["result",4]]],[11,"try_into","","",6,[[],["result",4]]],[11,"borrow","","",6,[[]]],[11,"borrow_mut","","",6,[[]]],[11,"type_id","","",6,[[],["typeid",3]]],[11,"vzip","","",6,[[]]],[11,"equivalent","","",6,[[]]],[11,"from","sn_routing::event","",21,[[]]],[11,"into","","",21,[[]]],[11,"try_from","","",21,[[],["result",4]]],[11,"try_into","","",21,[[],["result",4]]],[11,"borrow","","",21,[[]]],[11,"borrow_mut","","",21,[[]]],[11,"type_id","","",21,[[],["typeid",3]]],[11,"vzip","","",21,[[]]],[11,"from","","",22,[[]]],[11,"into","","",22,[[]]],[11,"try_from","","",22,[[],["result",4]]],[11,"try_into","","",22,[[],["result",4]]],[11,"borrow","","",22,[[]]],[11,"borrow_mut","","",22,[[]]],[11,"type_id","","",22,[[],["typeid",3]]],[11,"vzip","","",22,[[]]],[11,"from","","",7,[[]]],[11,"into","","",7,[[]]],[11,"to_owned","","",7,[[]]],[11,"clone_into","","",7,[[]]],[11,"try_from","","",7,[[],["result",4]]],[11,"try_into","","",7,[[],["result",4]]],[11,"borrow","","",7,[[]]],[11,"borrow_mut","","",7,[[]]],[11,"type_id","","",7,[[],["typeid",3]]],[11,"vzip","","",7,[[]]],[11,"equivalent","","",7,[[]]],[11,"from","","",9,[[]]],[11,"into","","",9,[[]]],[11,"try_from","","",9,[[],["result",4]]],[11,"try_into","","",9,[[],["result",4]]],[11,"borrow","","",9,[[]]],[11,"borrow_mut","","",9,[[]]],[11,"type_id","","",9,[[],["typeid",3]]],[11,"vzip","","",9,[[]]],[11,"from","sn_routing::rng","",23,[[]]],[11,"into","","",23,[[]]],[11,"to_owned","","",23,[[]]],[11,"clone_into","","",23,[[]]],[11,"try_from","","",23,[[],["result",4]]],[11,"try_into","","",23,[[],["result",4]]],[11,"borrow","","",23,[[]]],[11,"borrow_mut","","",23,[[]]],[11,"type_id","","",23,[[],["typeid",3]]],[11,"vzip","","",23,[[]]],[11,"clear","","",23,[[]]],[11,"initialize","","",23,[[]]],[11,"clone","sn_routing","",0,[[],["config",3]]],[11,"serialize","","",0,[[],["result",4]]],[11,"fmt","","",0,[[["formatter",3]],[["error",3],["result",4]]]],[11,"default","","",0,[[],["config",3]]],[11,"clap","","",0,[[],["app",3]]],[11,"from_clap","","",0,[[["argmatches",3]],["config",3]]],[11,"deserialize","","",0,[[],[["result",4],["config",3]]]],[11,"eq","","",0,[[["config",3]]]],[11,"ne","","",0,[[["config",3]]]],[11,"next_u32","sn_routing::rng","",23,[[]]],[11,"next_u64","","",23,[[]]],[11,"fill_bytes","","",23,[[]]],[11,"try_fill_bytes","","",23,[[],[["result",4],["error",3]]]],[11,"clone","","",23,[[],["osrng",3]]],[11,"fmt","","",23,[[["formatter",3]],[["error",3],["result",4]]]],[11,"default","","",23,[[],["osrng",3]]],[11,"fmt","sn_routing","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"deref","","",1,[[]]],[11,"cmp","","",20,[[["prefix",3]],["ordering",4]]],[11,"cmp","","",1,[[["xorname",3]],["ordering",4]]],[11,"from_str","","",20,[[],[["result",4],["prefix",3]]]],[11,"not","","",1,[[],["xorname",3]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"eq","","",20,[[["prefix",3]]]],[11,"eq","","",1,[[["xorname",3]]]],[11,"ne","","",1,[[["xorname",3]]]],[11,"serialize","","",1,[[],["result",4]]],[11,"serialize","","",20,[[],["result",4]]],[11,"default","","",20,[[],["prefix",3]]],[11,"default","","",1,[[],["xorname",3]]],[11,"hash","","",1,[[]]],[11,"hash","","",20,[[]]],[11,"deserialize","","",1,[[],[["xorname",3],["result",4]]]],[11,"deserialize","","",20,[[],[["prefix",3],["result",4]]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",20,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",20,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"clone","","",20,[[],["prefix",3]]],[11,"clone","","",1,[[],["xorname",3]]],[11,"partial_cmp","","",20,[[["prefix",3]],[["ordering",4],["option",4]]]],[11,"partial_cmp","","",1,[[["xorname",3]],[["ordering",4],["option",4]]]],[11,"lt","","",1,[[["xorname",3]]]],[11,"le","","",1,[[["xorname",3]]]],[11,"gt","","",1,[[["xorname",3]]]],[11,"ge","","",1,[[["xorname",3]]]],[11,"as_ref","","",1,[[]]],[11,"as_ref","","",1,[[],["xorname",3]]],[11,"from","","",4,[[["error",4]]]],[11,"from","","",4,[[["error",6]]]],[11,"clone","sn_routing::event","",7,[[],["connected",4]]],[11,"clone","sn_routing","",6,[[],["srclocation",4]]],[11,"clone","","",5,[[],["dstlocation",4]]],[11,"clone","","",2,[[],["networkparams",3]]],[11,"clone","","",19,[[],["sectionproofchain",3]]],[11,"default","","",2,[[]]],[11,"default","","",3,[[]]],[11,"eq","sn_routing::event","",7,[[["connected",4]]]],[11,"ne","","",7,[[["connected",4]]]],[11,"eq","sn_routing","",6,[[["srclocation",4]]]],[11,"ne","","",6,[[["srclocation",4]]]],[11,"eq","","",5,[[["dstlocation",4]]]],[11,"ne","","",5,[[["dstlocation",4]]]],[11,"eq","","",19,[[["sectionproofchain",3]]]],[11,"ne","","",19,[[["sectionproofchain",3]]]],[11,"fmt","sn_routing::event","",7,[[["formatter",3]],["result",6]]],[11,"fmt","","",9,[[["formatter",3]],["result",6]]],[11,"fmt","sn_routing","",4,[[["formatter",3]],["result",6]]],[11,"fmt","","",6,[[["formatter",3]],["result",6]]],[11,"fmt","","",5,[[["formatter",3]],["result",6]]],[11,"fmt","","",2,[[["formatter",3]],["result",6]]],[11,"fmt","","",19,[[["formatter",3]],["result",6]]],[11,"fmt","","",4,[[["formatter",3]],["result",6]]],[11,"hash","","",6,[[]]],[11,"hash","","",5,[[]]],[11,"hash","","",19,[[]]],[11,"description","","",4,[[]]],[11,"cause","","",4,[[],[["error",8],["option",4]]]],[11,"source","","",4,[[],[["error",8],["option",4]]]],[11,"serialize","","",6,[[],["result",4]]],[11,"serialize","","",5,[[],["result",4]]],[11,"serialize","","",19,[[],["result",4]]],[11,"deserialize","","",6,[[],["result",4]]],[11,"deserialize","","",5,[[],["result",4]]],[11,"deserialize","","",19,[[],["result",4]]],[11,"read_or_construct_default","","Try and read the config off the disk first. If such a…",0,[[["option",4],["dirs",4]],[["error",4],["result",4],["config",3]]]],[11,"new","","Creates a new `Prefix` with the first `bit_count` bits of…",20,[[["xorname",3]],["prefix",3]]],[11,"name","","Returns the name of this prefix.",20,[[],["xorname",3]]],[11,"pushed","","Returns `self` with an appended bit: `0` if `bit` is…",20,[[],["prefix",3]]],[11,"popped","","Returns a prefix copying the first `bitcount() - 1` bits…",20,[[],["prefix",3]]],[11,"bit_count","","Returns the number of bits in the prefix.",20,[[]]],[11,"is_empty","","Returns `true` if this is the empty prefix, with no bits.",20,[[]]],[11,"is_compatible","","Returns `true` if `self` is a prefix of `other` or vice…",20,[[["prefix",3]]]],[11,"is_extension_of","","Returns `true` if `other` is compatible but strictly…",20,[[["prefix",3]]]],[11,"is_neighbour","","Returns `true` if the `other` prefix differs in exactly…",20,[[["prefix",3]]]],[11,"common_prefix","","Returns the number of common leading bits with the input…",20,[[["xorname",3]]]],[11,"matches","","Returns `true` if this is a prefix of the given `name`.",20,[[["xorname",3]]]],[11,"cmp_distance","","Compares the distance of `self` and `other` to `target`.…",20,[[["prefix",3],["xorname",3]],["ordering",4]]],[11,"cmp_breadth_first","","Compares the prefixes using breadth-first order. That is,…",20,[[["prefix",3]],["ordering",4]]],[11,"lower_bound","","Returns the smallest name matching the prefix",20,[[],["xorname",3]]],[11,"upper_bound","","Returns the largest name matching the prefix",20,[[],["xorname",3]]],[11,"range_inclusive","","Inclusive range from lower_bound to upper_bound",20,[[],[["xorname",3],["rangeinclusive",3]]]],[11,"is_covered_by","","Returns whether the namespace defined by `self` is covered…",20,[[]]],[11,"with_flipped_bit","","Returns the neighbouring prefix differing in the `i`-th…",20,[[],["prefix",3]]],[11,"substituted_in","","Returns the given `name` with first bits replaced by `self`",20,[[["xorname",3]],["xorname",3]]],[11,"sibling","","Returns the same prefix, with the last bit flipped, or…",20,[[],["prefix",3]]],[11,"ancestor","","Returns the ancestors of this prefix that has the given…",20,[[],["prefix",3]]],[11,"ancestors","","Returns an iterator that yields all ancestors of this…",20,[[],["ancestors",3]]],[11,"random","","Generate a random XorName",1,[[],["xorname",3]]],[11,"bit","","Returns `true` if the `i`-th bit is `1`.",1,[[]]],[11,"cmp_distance","","Compares the distance of the arguments to `self`. Returns…",1,[[["xorname",3]],["ordering",4]]],[11,"next","sn_routing::event","Read next message from the stream",21,[[]]],[11,"send","","Send a message using the bi-directional stream created by…",22,[[["bytes",3]]]],[11,"finish","","Gracefully finish current stream",22,[[]]]],"p":[[3,"TransportConfig"],[3,"XorName"],[3,"NetworkParams"],[3,"Config"],[4,"Error"],[4,"DstLocation"],[4,"SrcLocation"],[4,"Connected"],[13,"Relocate"],[4,"Event"],[13,"MessageReceived"],[13,"MemberJoined"],[13,"InfantJoined"],[13,"MemberLeft"],[13,"EldersChanged"],[13,"RelocationStarted"],[13,"ClientMessageReceived"],[3,"EventStream"],[3,"Routing"],[3,"SectionProofChain"],[3,"Prefix"],[3,"RecvStream"],[3,"SendStream"],[3,"MainRng"]]}\
}');
addSearchOptions(searchIndex);initSearch(searchIndex);