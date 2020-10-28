initSidebarItems({"constant":[["MIN_AGE","The minimum age a node can have. The Infants will start at age 4. This is to prevent frequent relocations during the beginning of a node's lifetime."],["XOR_NAME_LEN","Constant byte length of `XorName`."]],"enum":[["DstLocation","Message destination location."],["Error","Internal error."],["SrcLocation","Message source location."]],"mod":[["event","sn_routing events."]],"struct":[["Config","Routing configuration."],["EventStream","Stream of routing node events"],["NetworkParams","Network parameters: number of elders, recommended section size"],["Prefix","A section prefix, i.e. a sequence of bits specifying the part of the network's name space consisting of all names that start with this sequence."],["Routing","Interface for sending and receiving messages to and from other nodes, in the role of a full routing node."],["SectionProofChain","Chain of section BLS keys where every key is proven (signed) by the previous key, except the first one."],["TransportConfig","QuicP2p configurations"],["XorName","A 256-bit number, viewed as a point in XOR space."]],"type":[["Result","The type returned by the sn_routing message handling methods."]]});