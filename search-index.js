var searchIndex = JSON.parse('{\
"sn_routing":{"doc":"Peer implementation for a resilient decentralised network…","i":[[3,"SendStream","sn_routing","Stream of outgoing messages",null,null],[3,"TransportConfig","","QuicP2p configurations",null,null],[12,"hard_coded_contacts","","Hard Coded contacts",0,null],[12,"port","","Port we want to reserve for QUIC. If none supplied we\'ll…",0,null],[12,"ip","","IP address for the listener. If none supplied we\'ll use…",0,null],[12,"max_msg_size_allowed","","This is the maximum message size we\'ll allow the peer to…",0,null],[12,"idle_timeout_msec","","If we hear nothing from the peer in the given interval we…",0,null],[12,"keep_alive_interval_msec","","Interval to send keep-alives if we are idling so that the…",0,null],[12,"bootstrap_cache_dir","","Directory in which the bootstrap cache will be stored. If…",0,null],[12,"upnp_lease_duration","","Duration of a UPnP port mapping.",0,null],[12,"forward_port","","Specify if port forwarding via UPnP should be done or not",0,null],[12,"fresh","","Use a fresh config without re-using any config available…",0,null],[12,"clean","","Clean all existing config available on disk",0,null],[3,"Prefix","","A section prefix, i.e. a sequence of bits specifying the…",null,null],[3,"XorName","","A 256-bit number, viewed as a point in XOR space.",null,null],[12,"0","","",1,null],[17,"XOR_NAME_LEN","","Constant byte length of `XorName`.",null,null],[3,"Config","","Routing configuration.",null,null],[12,"first","","If true, configures the node to start a new network…",2,null],[12,"keypair","","The `Keypair` of the node or `None` for randomly generated…",2,null],[12,"transport_config","","Configuration for the underlying network transport.",2,null],[3,"EventStream","","Stream of routing node events",null,null],[3,"Routing","","Interface for sending and receiving messages to and from…",null,null],[3,"SectionProofChain","","Chain of section BLS keys where every key is proven…",null,null],[4,"Error","","Internal error.",null,null],[13,"FailedSignature","","",3,null],[13,"CannotRoute","","",3,null],[13,"Network","","",3,null],[13,"InvalidState","","",3,null],[13,"Bincode","","",3,null],[13,"InvalidSrcLocation","","",3,null],[13,"InvalidDstLocation","","",3,null],[13,"InvalidMessage","","",3,null],[13,"InvalidSignatureShare","","",3,null],[13,"MissingSecretKeyShare","","",3,null],[13,"FailedSend","","",3,null],[13,"InvalidVote","","",3,null],[4,"Event","","An Event raised by a `Node` or `Client` via its event…",null,null],[13,"MessageReceived","","Received a message.",4,null],[12,"content","sn_routing::Event","The content of the message.",5,null],[12,"src","","The source location that sent the message.",5,null],[12,"dst","","The destination location that receives the message.",5,null],[13,"PromotedToElder","sn_routing","The node has been promoted to elder",4,null],[13,"PromotedToAdult","","The node has been promoted to adult",4,null],[13,"Demoted","","The node has been demoted from elder",4,null],[13,"MemberJoined","","A new peer joined our section.",4,null],[12,"name","sn_routing::Event","Name of the node",6,null],[12,"previous_name","","Previous name before relocation or `None` if it is a new…",6,null],[12,"age","","Age of the node",6,null],[12,"startup_relocation","","Indication that is has been relocated during startup.",6,null],[13,"MemberLeft","sn_routing","A node left our section.",4,null],[12,"name","sn_routing::Event","Name of the node",7,null],[12,"age","","Age of the node",7,null],[13,"EldersChanged","sn_routing","The set of elders in our section has changed.",4,null],[12,"prefix","sn_routing::Event","The prefix of our section.",8,null],[12,"key","","The BLS public key of our section.",8,null],[12,"elders","","The set of elders of our section.",8,null],[13,"RelocationStarted","sn_routing","This node has started relocating to other section. Will be…",4,null],[12,"previous_name","sn_routing::Event","Previous name before relocation",9,null],[13,"Relocated","sn_routing","This node has completed relocation to other section.",4,null],[12,"previous_name","sn_routing::Event","Old name before the relocation.",10,null],[12,"new_keypair","","New keypair to be used after relocation.",10,null],[13,"RestartRequired","sn_routing","Disconnected or failed to connect - restart required.",4,null],[13,"ClientMessageReceived","","Received a message from a client node.",4,null],[12,"content","sn_routing::Event","The content of the message.",11,null],[12,"src","","The address of the client that sent the message.",11,null],[12,"send","","Stream to send messages back to the client that sent the…",11,null],[12,"recv","","Stream to receive more messages from the client on the…",11,null],[13,"ClientLost","sn_routing","Failed in sending a message to client, or connection to…",4,null],[4,"DstLocation","","Message destination location.",null,null],[13,"Node","","Destination is a single node with the given name.",12,null],[13,"Section","","Destination are the nodes of the section whose prefix…",12,null],[13,"Direct","","Destination is the node at the `ConnectionInfo` the…",12,null],[4,"SrcLocation","","Message source location.",null,null],[13,"Node","","A single node with the given name.",13,null],[13,"Section","","A section with the given prefix.",13,null],[11,"is_section","","Returns whether this location is a section.",13,[[]]],[11,"to_dst","","Returns this location as `DstLocation`",13,[[],["dstlocation",4]]],[11,"is_section","","Returns whether this location is a section.",12,[[]]],[11,"next","","Returns next event",14,[[]]],[11,"new","","Creates new node using the given config and bootstraps it…",15,[[["config",3]]]],[11,"set_joins_allowed","","Sets the JoinsAllowed flag.",15,[[]]],[11,"age","","Returns the current age of this node.",15,[[]]],[11,"public_key","","Returns the ed25519 public key of this node.",15,[[]]],[11,"sign","","Signs any data with the ed25519 key of this node.",15,[[]]],[11,"verify","","Verifies `signature` on `data` with the ed25519 public key…",15,[[["signature",3]]]],[11,"name","","The name of this node.",15,[[]]],[11,"our_connection_info","","Returns connection info of this node.",15,[[]]],[11,"our_prefix","","Prefix of our section",15,[[]]],[11,"matches_our_prefix","","Finds out if the given XorName matches our prefix.",15,[[["xorname",3]]]],[11,"is_elder","","Returns whether the node is Elder.",15,[[]]],[11,"our_elders","","Returns the information of all the current section elders.",15,[[]]],[11,"our_elders_sorted_by_distance_to","","Returns the elders of our section sorted by their distance…",15,[[["xorname",3]]]],[11,"our_adults","","Returns the information of all the current section adults.",15,[[]]],[11,"our_adults_sorted_by_distance_to","","Returns the adults of our section sorted by their distance…",15,[[["xorname",3]]]],[11,"our_section","","Returns the info about our section or `None` if we are not…",15,[[]]],[11,"neighbour_sections","","Returns the info about our neighbour sections.",15,[[]]],[11,"match_section","","Returns the info about the section matches the name.",15,[[["xorname",3]]]],[11,"send_message","","Send a message. Messages sent here, either section to…",15,[[["bytes",3],["srclocation",4],["dstlocation",4]]]],[11,"send_message_to_client","","Send a message to a client peer. Messages sent to a client…",15,[[["socketaddr",4],["bytes",3]]]],[11,"public_key_set","","Returns the current BLS public key set if this node has…",15,[[]]],[11,"secret_key_share","","Returns the current BLS secret key share or…",15,[[]]],[11,"sign_with_secret_key_share","","Signs `data` with the BLS secret key share of this node,…",15,[[]]],[11,"our_history","","Returns our section proof chain.",15,[[]]],[11,"our_index","","Returns our index in the current BLS group if this node is…",15,[[]]],[11,"new","","Creates new chain consisting of only one block.",16,[[["publickey",3]]]],[11,"first_key","","Returns the first key of the chain.",16,[[],["publickey",3]]],[11,"last_key","","Returns the last key of the chain.",16,[[],["publickey",3]]],[11,"keys","","Returns all the keys of the chain as a DoubleEndedIterator.",16,[[]]],[11,"has_key","","Returns whether this chain contains the given key.",16,[[["publickey",3]]]],[11,"index_of","","Returns the index of the key in the chain or `None` if not…",16,[[["publickey",3]],["option",4]]],[11,"slice","","Returns a subset of this chain specified by the given…",16,[[["rangebounds",8]]]],[11,"len","","Number of blocks in the chain (including the first block)",16,[[]]],[11,"last_key_index","","Index of the last key in the chain.",16,[[]]],[11,"self_verify","","Check that all the blocks in the chain except the first…",16,[[]]],[11,"check_trust","","Verify this proof chain against the given trusted keys.",16,[[],["truststatus",4]]],[6,"Result","","The type returned by the sn_routing message handling…",null,null],[17,"MIN_AGE","","The minimum age a node can have. The Infants will start at…",null,null],[17,"RECOMMENDED_SECTION_SIZE","","Recommended section size. sn_routing will keep adding…",null,null],[17,"ELDER_SIZE","","Number of elders per section.",null,null],[11,"from","","",17,[[]]],[11,"into","","",17,[[]]],[11,"borrow","","",17,[[]]],[11,"borrow_mut","","",17,[[]]],[11,"try_from","","",17,[[],["result",4]]],[11,"try_into","","",17,[[],["result",4]]],[11,"type_id","","",17,[[],["typeid",3]]],[11,"vzip","","",17,[[]]],[11,"from","","",0,[[]]],[11,"into","","",0,[[]]],[11,"to_owned","","",0,[[]]],[11,"clone_into","","",0,[[]]],[11,"borrow","","",0,[[]]],[11,"borrow_mut","","",0,[[]]],[11,"try_from","","",0,[[],["result",4]]],[11,"try_into","","",0,[[],["result",4]]],[11,"type_id","","",0,[[],["typeid",3]]],[11,"vzip","","",0,[[]]],[11,"equivalent","","",0,[[]]],[11,"from","","",18,[[]]],[11,"into","","",18,[[]]],[11,"to_owned","","",18,[[]]],[11,"clone_into","","",18,[[]]],[11,"borrow","","",18,[[]]],[11,"borrow_mut","","",18,[[]]],[11,"try_from","","",18,[[],["result",4]]],[11,"try_into","","",18,[[],["result",4]]],[11,"type_id","","",18,[[],["typeid",3]]],[11,"vzip","","",18,[[]]],[11,"equivalent","","",18,[[]]],[11,"from","","",1,[[]]],[11,"into","","",1,[[]]],[11,"to_owned","","",1,[[]]],[11,"clone_into","","",1,[[]]],[11,"to_string","","",1,[[],["string",3]]],[11,"borrow","","",1,[[]]],[11,"borrow_mut","","",1,[[]]],[11,"try_from","","",1,[[],["result",4]]],[11,"try_into","","",1,[[],["result",4]]],[11,"type_id","","",1,[[],["typeid",3]]],[11,"vzip","","",1,[[]]],[11,"equivalent","","",1,[[]]],[11,"from","","",2,[[]]],[11,"into","","",2,[[]]],[11,"borrow","","",2,[[]]],[11,"borrow_mut","","",2,[[]]],[11,"try_from","","",2,[[],["result",4]]],[11,"try_into","","",2,[[],["result",4]]],[11,"type_id","","",2,[[],["typeid",3]]],[11,"vzip","","",2,[[]]],[11,"from","","",14,[[]]],[11,"into","","",14,[[]]],[11,"borrow","","",14,[[]]],[11,"borrow_mut","","",14,[[]]],[11,"try_from","","",14,[[],["result",4]]],[11,"try_into","","",14,[[],["result",4]]],[11,"type_id","","",14,[[],["typeid",3]]],[11,"vzip","","",14,[[]]],[11,"from","","",15,[[]]],[11,"into","","",15,[[]]],[11,"borrow","","",15,[[]]],[11,"borrow_mut","","",15,[[]]],[11,"try_from","","",15,[[],["result",4]]],[11,"try_into","","",15,[[],["result",4]]],[11,"type_id","","",15,[[],["typeid",3]]],[11,"vzip","","",15,[[]]],[11,"from","","",16,[[]]],[11,"into","","",16,[[]]],[11,"to_owned","","",16,[[]]],[11,"clone_into","","",16,[[]]],[11,"borrow","","",16,[[]]],[11,"borrow_mut","","",16,[[]]],[11,"try_from","","",16,[[],["result",4]]],[11,"try_into","","",16,[[],["result",4]]],[11,"type_id","","",16,[[],["typeid",3]]],[11,"vzip","","",16,[[]]],[11,"equivalent","","",16,[[]]],[11,"from","","",3,[[]]],[11,"into","","",3,[[]]],[11,"to_string","","",3,[[],["string",3]]],[11,"borrow","","",3,[[]]],[11,"borrow_mut","","",3,[[]]],[11,"try_from","","",3,[[],["result",4]]],[11,"try_into","","",3,[[],["result",4]]],[11,"type_id","","",3,[[],["typeid",3]]],[11,"vzip","","",3,[[]]],[11,"as_fail","","",3,[[],["fail",8]]],[11,"from","","",4,[[]]],[11,"into","","",4,[[]]],[11,"borrow","","",4,[[]]],[11,"borrow_mut","","",4,[[]]],[11,"try_from","","",4,[[],["result",4]]],[11,"try_into","","",4,[[],["result",4]]],[11,"type_id","","",4,[[],["typeid",3]]],[11,"vzip","","",4,[[]]],[11,"from","","",12,[[]]],[11,"into","","",12,[[]]],[11,"to_owned","","",12,[[]]],[11,"clone_into","","",12,[[]]],[11,"borrow","","",12,[[]]],[11,"borrow_mut","","",12,[[]]],[11,"try_from","","",12,[[],["result",4]]],[11,"try_into","","",12,[[],["result",4]]],[11,"type_id","","",12,[[],["typeid",3]]],[11,"vzip","","",12,[[]]],[11,"equivalent","","",12,[[]]],[11,"from","","",13,[[]]],[11,"into","","",13,[[]]],[11,"to_owned","","",13,[[]]],[11,"clone_into","","",13,[[]]],[11,"borrow","","",13,[[]]],[11,"borrow_mut","","",13,[[]]],[11,"try_from","","",13,[[],["result",4]]],[11,"try_into","","",13,[[],["result",4]]],[11,"type_id","","",13,[[],["typeid",3]]],[11,"vzip","","",13,[[]]],[11,"equivalent","","",13,[[]]],[11,"clone","","",0,[[],["config",3]]],[11,"default","","",0,[[],["config",3]]],[11,"fmt","","",0,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",17,[[["formatter",3]],[["error",3],["result",4]]]],[11,"deserialize","","",0,[[],[["result",4],["config",3]]]],[11,"clap","","",0,[[],["app",3]]],[11,"from_clap","","",0,[[["argmatches",3]],["config",3]]],[11,"eq","","",0,[[["config",3]]]],[11,"ne","","",0,[[["config",3]]]],[11,"serialize","","",0,[[],["result",4]]],[11,"serialize","","",1,[[],["result",4]]],[11,"serialize","","",18,[[],["result",4]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",18,[[["formatter",3]],[["error",3],["result",4]]]],[11,"eq","","",18,[[["prefix",3]]]],[11,"eq","","",1,[[["xorname",3]]]],[11,"ne","","",1,[[["xorname",3]]]],[11,"from_str","","",18,[[],[["prefix",3],["result",4]]]],[11,"cmp","","",18,[[["prefix",3]],["ordering",4]]],[11,"cmp","","",1,[[["xorname",3]],["ordering",4]]],[11,"deserialize","","",18,[[],[["result",4],["prefix",3]]]],[11,"deserialize","","",1,[[],[["result",4],["xorname",3]]]],[11,"deref","","",1,[[]]],[11,"as_ref","","",1,[[],["xorname",3]]],[11,"as_ref","","",1,[[]]],[11,"partial_cmp","","",1,[[["xorname",3]],[["ordering",4],["option",4]]]],[11,"lt","","",1,[[["xorname",3]]]],[11,"le","","",1,[[["xorname",3]]]],[11,"gt","","",1,[[["xorname",3]]]],[11,"ge","","",1,[[["xorname",3]]]],[11,"partial_cmp","","",18,[[["prefix",3]],[["ordering",4],["option",4]]]],[11,"fmt","","",18,[[["formatter",3]],[["error",3],["result",4]]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"clone","","",1,[[],["xorname",3]]],[11,"clone","","",18,[[],["prefix",3]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"default","","",1,[[],["xorname",3]]],[11,"default","","",18,[[],["prefix",3]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"not","","",1,[[],["xorname",3]]],[11,"hash","","",18,[[]]],[11,"hash","","",1,[[]]],[11,"fmt","","",1,[[["formatter",3]],[["error",3],["result",4]]]],[11,"drop","","",15,[[]]],[11,"from","","",3,[[["error",4]]]],[11,"from","","",3,[[["error",6]]]],[11,"clone","","",13,[[],["srclocation",4]]],[11,"clone","","",12,[[],["dstlocation",4]]],[11,"clone","","",16,[[],["sectionproofchain",3]]],[11,"default","","",2,[[]]],[11,"eq","","",13,[[["srclocation",4]]]],[11,"ne","","",13,[[["srclocation",4]]]],[11,"eq","","",12,[[["dstlocation",4]]]],[11,"ne","","",12,[[["dstlocation",4]]]],[11,"eq","","",16,[[["sectionproofchain",3]]]],[11,"ne","","",16,[[["sectionproofchain",3]]]],[11,"fmt","","",3,[[["formatter",3]],["result",6]]],[11,"fmt","","",4,[[["formatter",3]],["result",6]]],[11,"fmt","","",13,[[["formatter",3]],["result",6]]],[11,"fmt","","",12,[[["formatter",3]],["result",6]]],[11,"fmt","","",2,[[["formatter",3]],["result",6]]],[11,"fmt","","",16,[[["formatter",3]],["result",6]]],[11,"fmt","","",3,[[["formatter",3]],["result",6]]],[11,"hash","","",13,[[]]],[11,"hash","","",12,[[]]],[11,"hash","","",16,[[]]],[11,"source","","",3,[[],[["option",4],["error",8]]]],[11,"serialize","","",13,[[],["result",4]]],[11,"serialize","","",12,[[],["result",4]]],[11,"serialize","","",16,[[],["result",4]]],[11,"deserialize","","",13,[[],["result",4]]],[11,"deserialize","","",12,[[],["result",4]]],[11,"deserialize","","",16,[[],["result",4]]],[11,"send_user_msg","","Send a message using the stream created by the initiator",17,[[["bytes",3]]]],[11,"send","","Send a wire message",17,[[["wiremsg",4]]]],[11,"finish","","Gracefully finish current stream",17,[[]]],[11,"read_or_construct_default","","Try and read the config off the disk first. If such a…",0,[[["path",3],["option",4]],[["config",3],["error",4],["result",4]]]],[11,"clear_config_from_disk","","Clear all configuration files from disk",0,[[["path",3],["option",4]],[["error",4],["result",4]]]],[11,"new","","Creates a new `Prefix` with the first `bit_count` bits of…",18,[[["xorname",3]],["prefix",3]]],[11,"name","","Returns the name of this prefix.",18,[[],["xorname",3]]],[11,"pushed","","Returns `self` with an appended bit: `0` if `bit` is…",18,[[],["prefix",3]]],[11,"popped","","Returns a prefix copying the first `bitcount() - 1` bits…",18,[[],["prefix",3]]],[11,"bit_count","","Returns the number of bits in the prefix.",18,[[]]],[11,"is_empty","","Returns `true` if this is the empty prefix, with no bits.",18,[[]]],[11,"is_compatible","","Returns `true` if `self` is a prefix of `other` or vice…",18,[[["prefix",3]]]],[11,"is_extension_of","","Returns `true` if `other` is compatible but strictly…",18,[[["prefix",3]]]],[11,"is_neighbour","","Returns `true` if the `other` prefix differs in exactly…",18,[[["prefix",3]]]],[11,"common_prefix","","Returns the number of common leading bits with the input…",18,[[["xorname",3]]]],[11,"matches","","Returns `true` if this is a prefix of the given `name`.",18,[[["xorname",3]]]],[11,"cmp_distance","","Compares the distance of `self` and `other` to `target`.…",18,[[["prefix",3],["xorname",3]],["ordering",4]]],[11,"cmp_breadth_first","","Compares the prefixes using breadth-first order. That is,…",18,[[["prefix",3]],["ordering",4]]],[11,"lower_bound","","Returns the smallest name matching the prefix",18,[[],["xorname",3]]],[11,"upper_bound","","Returns the largest name matching the prefix",18,[[],["xorname",3]]],[11,"range_inclusive","","Inclusive range from lower_bound to upper_bound",18,[[],[["rangeinclusive",3],["xorname",3]]]],[11,"is_covered_by","","Returns whether the namespace defined by `self` is covered…",18,[[]]],[11,"with_flipped_bit","","Returns the neighbouring prefix differing in the `i`-th…",18,[[],["prefix",3]]],[11,"substituted_in","","Returns the given `name` with first bits replaced by `self`",18,[[["xorname",3]],["xorname",3]]],[11,"sibling","","Returns the same prefix, with the last bit flipped, or…",18,[[],["prefix",3]]],[11,"ancestor","","Returns the ancestors of this prefix that has the given…",18,[[],["prefix",3]]],[11,"ancestors","","Returns an iterator that yields all ancestors of this…",18,[[],["ancestors",3]]],[11,"random","","Generate a random XorName",1,[[],["xorname",3]]],[11,"bit","","Returns `true` if the `i`-th bit is `1`.",1,[[]]],[11,"cmp_distance","","Compares the distance of the arguments to `self`. Returns…",1,[[["xorname",3]],["ordering",4]]]],"p":[[3,"TransportConfig"],[3,"XorName"],[3,"Config"],[4,"Error"],[4,"Event"],[13,"MessageReceived"],[13,"MemberJoined"],[13,"MemberLeft"],[13,"EldersChanged"],[13,"RelocationStarted"],[13,"Relocated"],[13,"ClientMessageReceived"],[4,"DstLocation"],[4,"SrcLocation"],[3,"EventStream"],[3,"Routing"],[3,"SectionProofChain"],[3,"SendStream"],[3,"Prefix"]]}\
}');
addSearchOptions(searchIndex);initSearch(searchIndex);