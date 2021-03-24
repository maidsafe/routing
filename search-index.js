var searchIndex = JSON.parse('{\
"sn_routing":{"doc":"Peer implementation for a resilient decentralised network …","i":[[3,"SendStream","sn_routing","Stream of outgoing messages",null,null],[3,"TransportConfig","","QuicP2p configurations",null,null],[12,"hard_coded_contacts","","Hard Coded contacts",0,null],[12,"local_port","","Port we want to reserve for QUIC. If none supplied we\'ll …",0,null],[12,"local_ip","","IP address for the listener. If none is supplied and …",0,null],[12,"forward_port","","Specify if port forwarding via UPnP should be done or …",0,null],[12,"external_port","","External port number assigned to the socket address of …",0,null],[12,"external_ip","","External IP address of the computer on the WAN. This …",0,null],[12,"max_msg_size_allowed","","This is the maximum message size we\'ll allow the peer to …",0,null],[12,"idle_timeout_msec","","If we hear nothing from the peer in the given interval we …",0,null],[12,"keep_alive_interval_msec","","Interval to send keep-alives if we are idling so that the …",0,null],[12,"bootstrap_cache_dir","","Directory in which the bootstrap cache will be stored. If …",0,null],[12,"upnp_lease_duration","","Duration of a UPnP port mapping.",0,null],[3,"Prefix","","A section prefix, i.e. a sequence of bits specifying the …",null,null],[3,"XorName","","A 256-bit number, viewed as a point in XOR space.",null,null],[12,"0","","",1,null],[17,"XOR_NAME_LEN","","Constant byte length of <code>XorName</code>.",null,null],[4,"Error","","Internal error.",null,null],[13,"FailedSignature","","",2,null],[13,"CannotRoute","","",2,null],[13,"Network","","",2,null],[13,"InvalidState","","",2,null],[13,"Bincode","","",2,null],[13,"InvalidSrcLocation","","",2,null],[13,"InvalidDstLocation","","",2,null],[13,"InvalidMessage","","",2,null],[13,"InvalidSignatureShare","","",2,null],[13,"MissingSecretKeyShare","","",2,null],[13,"FailedSend","","",2,null],[13,"InvalidVote","","",2,null],[13,"InvalidSectionChain","","",2,null],[13,"Messaging","","",2,null],[6,"Result","","The type returned by the sn_routing message handling …",null,null],[3,"Elders","","",null,null],[12,"prefix","","The prefix of the section.",3,null],[12,"key","","The BLS public key of a section.",3,null],[12,"elders","","The set of elders of a section.",3,null],[4,"Event","","An Event raised by a <code>Node</code> or <code>Client</code> via its event sender.",null,null],[13,"Genesis","","This is the very first node in a network.",4,null],[13,"MessageReceived","","Received a message.",4,null],[12,"content","sn_routing::Event","The content of the message.",5,null],[12,"src","","The source location that sent the message.",5,null],[12,"dst","","The destination location that receives the message.",5,null],[13,"MemberJoined","sn_routing","A new peer joined our section.",4,null],[12,"name","sn_routing::Event","Name of the node",6,null],[12,"previous_name","","Previous name before relocation or <code>None</code> if it is a new …",6,null],[12,"age","","Age of the node",6,null],[13,"MemberLeft","sn_routing","A node left our section.",4,null],[12,"name","sn_routing::Event","Name of the node",7,null],[12,"age","","Age of the node",7,null],[13,"EldersChanged","sn_routing","The set of elders in our section has changed.",4,null],[12,"elders","sn_routing::Event","The Elders of our section.",8,null],[12,"sibling_elders","","The Elders of the sibling section, if this event is fired …",8,null],[12,"self_status_change","","Promoted, demoted or no change?",8,null],[13,"RelocationStarted","sn_routing","This node has started relocating to other section. Will …",4,null],[12,"previous_name","sn_routing::Event","Previous name before relocation",9,null],[13,"Relocated","sn_routing","This node has completed relocation to other section.",4,null],[12,"previous_name","sn_routing::Event","Old name before the relocation.",10,null],[12,"new_keypair","","New keypair to be used after relocation.",10,null],[13,"RestartRequired","sn_routing","Disconnected or failed to connect - restart required.",4,null],[13,"ClientMessageReceived","","Received a message from a client node.",4,null],[12,"msg","sn_routing::Event","The content of the message.",11,null],[12,"user","","The SocketAddr and PublicKey that sent the message. …",11,null],[13,"ClientLost","sn_routing","Failed in sending a message to client, or connection to …",4,null],[4,"NodeElderChange","","A flag in EldersChanged event, indicating whether the …",null,null],[13,"Promoted","","The node was promoted to Elder.",12,null],[13,"Demoted","","The node was demoted to Adult.",12,null],[13,"None","","There was no change to the node.",12,null],[3,"Config","","Routing configuration.",null,null],[12,"first","","If true, configures the node to start a new network …",13,null],[12,"keypair","","The <code>Keypair</code> of the node or <code>None</code> for randomly generated …",13,null],[12,"transport_config","","Configuration for the underlying network transport.",13,null],[3,"EventStream","","Stream of routing node events",null,null],[3,"Routing","","Interface for sending and receiving messages to and from …",null,null],[3,"SectionChain","","Chain of section BLS keys where every key is proven …",null,null],[4,"SectionChainError","","Error resulting from operations on <code>SectionChain</code>.",null,null],[13,"FailedSignature","","",14,null],[13,"KeyNotFound","","",14,null],[13,"Untrusted","","",14,null],[13,"InvalidOperation","","",14,null],[17,"MIN_AGE","","The minimum age a node can have. The Infants will start …",null,null],[17,"RECOMMENDED_SECTION_SIZE","","Recommended section size. sn_routing will keep adding …",null,null],[17,"ELDER_SIZE","","Number of elders per section.",null,null],[11,"from","","",15,[[]]],[11,"into","","",15,[[]]],[11,"borrow","","",15,[[]]],[11,"borrow_mut","","",15,[[]]],[11,"try_from","","",15,[[],["result",4]]],[11,"try_into","","",15,[[],["result",4]]],[11,"type_id","","",15,[[],["typeid",3]]],[11,"vzip","","",15,[[]]],[11,"from","","",0,[[]]],[11,"into","","",0,[[]]],[11,"to_owned","","",0,[[]]],[11,"clone_into","","",0,[[]]],[11,"borrow","","",0,[[]]],[11,"borrow_mut","","",0,[[]]],[11,"try_from","","",0,[[],["result",4]]],[11,"try_into","","",0,[[],["result",4]]],[11,"type_id","","",0,[[],["typeid",3]]],[11,"vzip","","",0,[[]]],[11,"equivalent","","",0,[[]]],[11,"from","","",16,[[]]],[11,"into","","",16,[[]]],[11,"to_owned","","",16,[[]]],[11,"clone_into","","",16,[[]]],[11,"borrow","","",16,[[]]],[11,"borrow_mut","","",16,[[]]],[11,"try_from","","",16,[[],["result",4]]],[11,"try_into","","",16,[[],["result",4]]],[11,"type_id","","",16,[[],["typeid",3]]],[11,"vzip","","",16,[[]]],[11,"equivalent","","",16,[[]]],[11,"from","","",1,[[]]],[11,"into","","",1,[[]]],[11,"to_owned","","",1,[[]]],[11,"clone_into","","",1,[[]]],[11,"to_string","","",1,[[],["string",3]]],[11,"borrow","","",1,[[]]],[11,"borrow_mut","","",1,[[]]],[11,"try_from","","",1,[[],["result",4]]],[11,"try_into","","",1,[[],["result",4]]],[11,"type_id","","",1,[[],["typeid",3]]],[11,"vzip","","",1,[[]]],[11,"equivalent","","",1,[[]]],[11,"from","","",2,[[]]],[11,"into","","",2,[[]]],[11,"to_string","","",2,[[],["string",3]]],[11,"borrow","","",2,[[]]],[11,"borrow_mut","","",2,[[]]],[11,"try_from","","",2,[[],["result",4]]],[11,"try_into","","",2,[[],["result",4]]],[11,"type_id","","",2,[[],["typeid",3]]],[11,"vzip","","",2,[[]]],[11,"as_fail","","",2,[[],["fail",8]]],[11,"from","","",12,[[]]],[11,"into","","",12,[[]]],[11,"borrow","","",12,[[]]],[11,"borrow_mut","","",12,[[]]],[11,"try_from","","",12,[[],["result",4]]],[11,"try_into","","",12,[[],["result",4]]],[11,"type_id","","",12,[[],["typeid",3]]],[11,"vzip","","",12,[[]]],[11,"from","","",3,[[]]],[11,"into","","",3,[[]]],[11,"to_owned","","",3,[[]]],[11,"clone_into","","",3,[[]]],[11,"borrow","","",3,[[]]],[11,"borrow_mut","","",3,[[]]],[11,"try_from","","",3,[[],["result",4]]],[11,"try_into","","",3,[[],["result",4]]],[11,"type_id","","",3,[[],["typeid",3]]],[11,"vzip","","",3,[[]]],[11,"from","","",4,[[]]],[11,"into","","",4,[[]]],[11,"borrow","","",4,[[]]],[11,"borrow_mut","","",4,[[]]],[11,"try_from","","",4,[[],["result",4]]],[11,"try_into","","",4,[[],["result",4]]],[11,"type_id","","",4,[[],["typeid",3]]],[11,"vzip","","",4,[[]]],[11,"from","","",17,[[]]],[11,"into","","",17,[[]]],[11,"borrow","","",17,[[]]],[11,"borrow_mut","","",17,[[]]],[11,"try_from","","",17,[[],["result",4]]],[11,"try_into","","",17,[[],["result",4]]],[11,"type_id","","",17,[[],["typeid",3]]],[11,"vzip","","",17,[[]]],[11,"from","","",13,[[]]],[11,"into","","",13,[[]]],[11,"borrow","","",13,[[]]],[11,"borrow_mut","","",13,[[]]],[11,"try_from","","",13,[[],["result",4]]],[11,"try_into","","",13,[[],["result",4]]],[11,"type_id","","",13,[[],["typeid",3]]],[11,"vzip","","",13,[[]]],[11,"from","","",18,[[]]],[11,"into","","",18,[[]]],[11,"borrow","","",18,[[]]],[11,"borrow_mut","","",18,[[]]],[11,"try_from","","",18,[[],["result",4]]],[11,"try_into","","",18,[[],["result",4]]],[11,"type_id","","",18,[[],["typeid",3]]],[11,"vzip","","",18,[[]]],[11,"from","","",19,[[]]],[11,"into","","",19,[[]]],[11,"to_owned","","",19,[[]]],[11,"clone_into","","",19,[[]]],[11,"borrow","","",19,[[]]],[11,"borrow_mut","","",19,[[]]],[11,"try_from","","",19,[[],["result",4]]],[11,"try_into","","",19,[[],["result",4]]],[11,"type_id","","",19,[[],["typeid",3]]],[11,"vzip","","",19,[[]]],[11,"equivalent","","",19,[[]]],[11,"from","","",14,[[]]],[11,"into","","",14,[[]]],[11,"to_string","","",14,[[],["string",3]]],[11,"borrow","","",14,[[]]],[11,"borrow_mut","","",14,[[]]],[11,"try_from","","",14,[[],["result",4]]],[11,"try_into","","",14,[[],["result",4]]],[11,"type_id","","",14,[[],["typeid",3]]],[11,"vzip","","",14,[[]]],[11,"equivalent","","",14,[[]]],[11,"as_fail","","",14,[[],["fail",8]]],[11,"default","","",0,[[],["config",3]]],[11,"clone","","",0,[[],["config",3]]],[11,"eq","","",0,[[["config",3]]]],[11,"ne","","",0,[[["config",3]]]],[11,"deserialize","","",0,[[],[["result",4],["config",3]]]],[11,"fmt","","",0,[[["formatter",3]],[["result",4],["error",3]]]],[11,"fmt","","",15,[[["formatter",3]],[["result",4],["error",3]]]],[11,"clap","","",0,[[],["app",3]]],[11,"from_clap","","",0,[[["argmatches",3]],["config",3]]],[11,"serialize","","",0,[[],["result",4]]],[11,"cmp","","",1,[[["xorname",3]],["ordering",4]]],[11,"cmp","","",16,[[["prefix",3]],["ordering",4]]],[11,"eq","","",1,[[["xorname",3]]]],[11,"ne","","",1,[[["xorname",3]]]],[11,"eq","","",16,[[["prefix",3]]]],[11,"fmt","","",1,[[["formatter",3]],[["result",4],["error",3]]]],[11,"fmt","","",16,[[["formatter",3]],[["result",4],["error",3]]]],[11,"clone","","",1,[[],["xorname",3]]],[11,"clone","","",16,[[],["prefix",3]]],[11,"fmt","","",1,[[["formatter",3]],[["result",4],["error",3]]]],[11,"as_ref","","",1,[[],["xorname",3]]],[11,"as_ref","","",1,[[]]],[11,"fmt","","",1,[[["formatter",3]],[["result",4],["error",3]]]],[11,"serialize","","",1,[[],["result",4]]],[11,"serialize","","",16,[[],["result",4]]],[11,"hash","","",16,[[]]],[11,"hash","","",1,[[]]],[11,"default","","",16,[[],["prefix",3]]],[11,"default","","",1,[[],["xorname",3]]],[11,"fmt","","",1,[[["formatter",3]],[["result",4],["error",3]]]],[11,"deserialize","","",16,[[],[["result",4],["prefix",3]]]],[11,"deserialize","","",1,[[],[["xorname",3],["result",4]]]],[11,"fmt","","",1,[[["formatter",3]],[["result",4],["error",3]]]],[11,"fmt","","",16,[[["formatter",3]],[["result",4],["error",3]]]],[11,"from_str","","",16,[[],[["result",4],["prefix",3]]]],[11,"deref","","",1,[[]]],[11,"not","","",1,[[],["xorname",3]]],[11,"partial_cmp","","",16,[[["prefix",3]],[["ordering",4],["option",4]]]],[11,"partial_cmp","","",1,[[["xorname",3]],[["ordering",4],["option",4]]]],[11,"lt","","",1,[[["xorname",3]]]],[11,"le","","",1,[[["xorname",3]]]],[11,"gt","","",1,[[["xorname",3]]]],[11,"ge","","",1,[[["xorname",3]]]],[11,"from","","",1,[[["publickey",4]],["xorname",3]]],[11,"drop","","",18,[[]]],[11,"from","","",2,[[["error",4]]]],[11,"from","","",2,[[["error",6]]]],[11,"from","","",2,[[["sectionchainerror",4]]]],[11,"from","","",2,[[["error",4]]]],[11,"clone","","",3,[[],["elders",3]]],[11,"clone","","",19,[[],["sectionchain",3]]],[11,"default","","",13,[[]]],[11,"eq","","",3,[[["elders",3]]]],[11,"ne","","",3,[[["elders",3]]]],[11,"eq","","",19,[[["sectionchain",3]]]],[11,"ne","","",19,[[["sectionchain",3]]]],[11,"eq","","",14,[[["error",4]]]],[11,"fmt","","",2,[[["formatter",3]],["result",6]]],[11,"fmt","","",12,[[["formatter",3]],["result",6]]],[11,"fmt","","",3,[[["formatter",3]],["result",6]]],[11,"fmt","","",4,[[["formatter",3]],["result",6]]],[11,"fmt","","",13,[[["formatter",3]],["result",6]]],[11,"fmt","","",19,[[["formatter",3]],["result",6]]],[11,"fmt","","",14,[[["formatter",3]],["result",6]]],[11,"fmt","","",2,[[["formatter",3]],["result",6]]],[11,"fmt","","",14,[[["formatter",3]],["result",6]]],[11,"hash","","",19,[[]]],[11,"source","","",2,[[],[["error",8],["option",4]]]],[11,"serialize","","",19,[[],["result",4]]],[11,"deserialize","","",19,[[],["result",4]]],[11,"send_user_msg","","Send a message using the stream created by the initiator",15,[[["bytes",3]]]],[11,"send","","Send a wire message",15,[[["wiremsg",4]]]],[11,"finish","","Gracefully finish current stream",15,[[]]],[11,"new","","Creates a new <code>Prefix</code> with the first <code>bit_count</code> bits of <code>name</code>…",16,[[["xorname",3]],["prefix",3]]],[11,"name","","Returns the name of this prefix.",16,[[],["xorname",3]]],[11,"pushed","","Returns <code>self</code> with an appended bit: <code>0</code> if <code>bit</code> is <code>false</code>, and …",16,[[],["prefix",3]]],[11,"popped","","Returns a prefix copying the first <code>bitcount() - 1</code> bits …",16,[[],["prefix",3]]],[11,"bit_count","","Returns the number of bits in the prefix.",16,[[]]],[11,"is_empty","","Returns <code>true</code> if this is the empty prefix, with no bits.",16,[[]]],[11,"is_compatible","","Returns <code>true</code> if <code>self</code> is a prefix of <code>other</code> or vice versa.",16,[[["prefix",3]]]],[11,"is_extension_of","","Returns <code>true</code> if <code>other</code> is compatible but strictly shorter …",16,[[["prefix",3]]]],[11,"is_neighbour","","Returns <code>true</code> if the <code>other</code> prefix differs in exactly one …",16,[[["prefix",3]]]],[11,"common_prefix","","Returns the number of common leading bits with the input …",16,[[["xorname",3]]]],[11,"matches","","Returns <code>true</code> if this is a prefix of the given <code>name</code>.",16,[[["xorname",3]]]],[11,"cmp_distance","","Compares the distance of <code>self</code> and <code>other</code> to <code>target</code>. …",16,[[["prefix",3],["xorname",3]],["ordering",4]]],[11,"cmp_breadth_first","","Compares the prefixes using breadth-first order. That is, …",16,[[["prefix",3]],["ordering",4]]],[11,"lower_bound","","Returns the smallest name matching the prefix",16,[[],["xorname",3]]],[11,"upper_bound","","Returns the largest name matching the prefix",16,[[],["xorname",3]]],[11,"range_inclusive","","Inclusive range from lower_bound to upper_bound",16,[[],[["rangeinclusive",3],["xorname",3]]]],[11,"is_covered_by","","Returns whether the namespace defined by <code>self</code> is covered …",16,[[]]],[11,"with_flipped_bit","","Returns the neighbouring prefix differing in the <code>i</code>-th bit …",16,[[],["prefix",3]]],[11,"substituted_in","","Returns the given <code>name</code> with first bits replaced by <code>self</code>",16,[[["xorname",3]],["xorname",3]]],[11,"sibling","","Returns the same prefix, with the last bit flipped, or …",16,[[],["prefix",3]]],[11,"ancestor","","Returns the ancestors of this prefix that has the given …",16,[[],["prefix",3]]],[11,"ancestors","","Returns an iterator that yields all ancestors of this …",16,[[],["ancestors",3]]],[11,"from_content","","Generate a XorName for the given content (for …",1,[[],["xorname",3]]],[11,"random","","Generate a random XorName",1,[[],["xorname",3]]],[11,"bit","","Returns <code>true</code> if the <code>i</code>-th bit is <code>1</code>.",1,[[]]],[11,"cmp_distance","","Compares the distance of the arguments to <code>self</code>. Returns …",1,[[["xorname",3]],["ordering",4]]],[11,"key","","The BLS public key",3,[[],["publickey",4]]],[11,"name","","The BLS based name",3,[[],["xorname",3]]],[11,"address","","The prefix based address",3,[[],["xorname",3]]],[11,"next","","Returns next event",17,[[]]],[11,"new","","Creates new node using the given config and bootstraps it …",18,[[["config",3]]]],[11,"set_joins_allowed","","Sets the JoinsAllowed flag.",18,[[]]],[11,"age","","Returns the current age of this node.",18,[[]]],[11,"public_key","","Returns the ed25519 public key of this node.",18,[[]]],[11,"sign_as_node","","Signs <code>data</code> with the ed25519 key of this node.",18,[[]]],[11,"sign_as_elder","","Signs <code>data</code> with the BLS secret key share of this node, if …",18,[[["publickey",3]]]],[11,"verify","","Verifies <code>signature</code> on <code>data</code> with the ed25519 public key of …",18,[[["signature",3]]]],[11,"name","","The name of this node.",18,[[]]],[11,"our_connection_info","","Returns connection info of this node.",18,[[],["socketaddr",4]]],[11,"section_chain","","Returns the Section Proof Chain",18,[[]]],[11,"our_prefix","","Prefix of our section",18,[[]]],[11,"matches_our_prefix","","Finds out if the given XorName matches our prefix.",18,[[["xorname",3]]]],[11,"is_elder","","Returns whether the node is Elder.",18,[[]]],[11,"our_elders","","Returns the information of all the current section elders.",18,[[]]],[11,"our_elders_sorted_by_distance_to","","Returns the elders of our section sorted by their …",18,[[["xorname",3]]]],[11,"our_adults","","Returns the information of all the current section adults.",18,[[]]],[11,"our_adults_sorted_by_distance_to","","Returns the adults of our section sorted by their …",18,[[["xorname",3]]]],[11,"our_section","","Returns the info about our section or <code>None</code> if we are not …",18,[[]]],[11,"other_sections","","Returns the info about other sections in the network …",18,[[]]],[11,"section_key","","Returns the last known public key of the section with …",18,[[["prefix",3]]]],[11,"matching_section","","Returns the info about the section matching the name.",18,[[["xorname",3]]]],[11,"send_message","","Send a message. Messages sent here, either section to …",18,[[["itinerary",3],["bytes",3]]]],[11,"public_key_set","","Returns the current BLS public key set if this node has …",18,[[]]],[11,"our_history","","Returns our section proof chain.",18,[[]]],[11,"our_index","","Returns our index in the current BLS group if this node …",18,[[]]],[11,"new","","Creates a new chain consisting of only one block.",19,[[["publickey",3]]]],[11,"insert","","Insert new key into the chain. <code>parent_key</code> must exists in …",19,[[["publickey",3],["publickey",3],["signature",3]],[["error",4],["result",4]]]],[11,"merge","","Merges two chains into one.",19,[[],[["error",4],["result",4]]]],[11,"minimize","","Creates a minimal sub-chain of <code>self</code> that contains all …",19,[[],[["error",4],["result",4]]]],[11,"truncate","","Returns a sub-chain of <code>self</code> truncated to the last <code>count</code> …",19,[[]]],[11,"extend","","Returns the smallest super-chain of <code>self</code> that would be …",19,[[["publickey",3]],[["error",4],["result",4]]]],[11,"keys","","Iterator over all the keys in the chain in order.",19,[[]]],[11,"root_key","","Returns the root key of this chain. This is the first key …",19,[[],["publickey",3]]],[11,"last_key","","Returns the last key of this chain.",19,[[],["publickey",3]]],[11,"has_key","","Returns whether <code>key</code> is present in this chain.",19,[[["publickey",3]]]],[11,"check_trust","","Given a collection of keys that are already trusted, …",19,[[]]],[11,"cmp_by_position","","Compare the two keys by their position in the chain. The …",19,[[["publickey",3]],["ordering",4]]],[11,"len","","Returns the number of blocks in the chain. This is always …",19,[[]]],[11,"main_branch_len","","Returns the number of block on the main branch of the …",19,[[]]]],"p":[[3,"TransportConfig"],[3,"XorName"],[4,"Error"],[3,"Elders"],[4,"Event"],[13,"MessageReceived"],[13,"MemberJoined"],[13,"MemberLeft"],[13,"EldersChanged"],[13,"RelocationStarted"],[13,"Relocated"],[13,"ClientMessageReceived"],[4,"NodeElderChange"],[3,"Config"],[4,"SectionChainError"],[3,"SendStream"],[3,"Prefix"],[3,"EventStream"],[3,"Routing"],[3,"SectionChain"]]}\
}');
addSearchOptions(searchIndex);initSearch(searchIndex);