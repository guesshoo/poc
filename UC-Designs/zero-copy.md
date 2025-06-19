In this context “zero-copy” doesn’t mean you avoid that one initial read from the OS into your TCP buffer—after all, you have to get the bytes off the wire somehow—but rather that you never do a second copy or allocation to turn those bytes into your in-memory data structures.

With **Protocol Buffers**, when you call `Message::decode(&buf)`, the library will:

1. Walk the buffer,
2. Allocate a fresh Rust struct (and often heap-allocate sub-fields),
3. Copy data (strings, nested messages, byte arrays) out of the wire buffer into your new objects.

By contrast, **FlatBuffers** arrange their on-disk/wire format so that it **is** the in-memory format. You:

1. Read the entire FlatBuffer blob into a single `&[u8]` (or memory-map it),
2. Call the generated accessors on that slice,
3. Instantly get references or integers out of it without any extra `malloc` or `memcpy`.

> “The main distinction of FlatBuffers is that it implements zero-copy deserialization: it does not need to create objects or reserve new memory areas to parse the information, because it always works with the information in binary within a memory or disk area.” ([jeronimo.dev][1])
>
> “FlatBuffers has a same representation for its in-memory layout and wire format, without unpacking when reading data.” ([github.com][2])

So the “zero-copy” benefit is **after** the socket gives you a single byte buffer, you never duplicate or re-marshal those bytes again—your application code can just call into the FlatBuffers API and read fields in-place. This can cut both CPU and GC/allocator pressure in very high-throughput systems.

[1]: https://www.jeronimo.dev/java-serialization-with-flatbuffers/?utm_source=chatgpt.com "Java Serialization with Flatbuffers | Spartan Blog - Jerónimo"
[2]: https://github.com/protocolbuffers/protobuf/issues/3296?utm_source=chatgpt.com "The fundamental difference between PB and FB is what ? #3296"
