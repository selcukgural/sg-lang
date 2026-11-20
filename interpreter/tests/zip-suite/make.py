# The archives of `made/`, most of them wrong on purpose, for the checks of `ZipReader` (choices
# 1491, 1501-1517). Run from this folder: python3 -I make.py made
import struct, sys, zlib, os, io, zipfile

out = sys.argv[1]

def raw_deflate(data, level=9):
    c = zlib.compressobj(level, zlib.DEFLATED, -15)
    return c.compress(data) + c.flush()

def local(name, method, crc, csize, usize, flags=0, extra=b""):
    return struct.pack("<IHHHHHIIIHH", 0x04034B50, 20, flags, method, 0, 0x21, crc, csize, usize, len(name), len(extra)) + name + extra

def central(name, method, crc, csize, usize, offset, flags=0, extra=b""):
    return struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, flags, method, 0, 0x21, crc, csize, usize, len(name), len(extra), 0, 0, 0, 0, offset) + name + extra

def end(count, size, offset):
    return struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, count, count, size, offset, 0)

def archive(entries):
    """entries: (name, method, data_bytes_as_written, crc, usize, flags) -> a plain archive."""
    body = b""
    cd = b""
    for name, method, payload, crc, usize, flags in entries:
        off = len(body)
        body += local(name, method, crc, len(payload), usize, flags) + payload
        cd += central(name, method, crc, len(payload), usize, off, flags)
    return body + cd + end(len(entries), len(cd), len(body))

def write(name, data):
    with open(os.path.join(out, name), "wb") as f:
        f.write(data)

def stored(name, data, flags=0):
    return (name, 0, data, zlib.crc32(data), len(data), flags)

def deflated(name, data):
    return (name, 8, raw_deflate(data), zlib.crc32(data), len(data), 0)

hello = b"hello, zip\n"

# Names that could lead out of a folder (1502).
for i, bad in enumerate([b"../evil.txt", b"a/../../evil.txt", b"/etc/passwd", b"a\\b.txt", b"C:evil.txt", b"c:/evil.txt",
                         b"file.txt:stream", b"aux.txt", b"dir/NUL", b"lpt9", b"Com1 .txt", b"", b"a\x00b"]):
    write(f"insecure-{i:02}.zip", archive([stored(b"ok.txt", hello), stored(bad, hello)]))
# Names that are fine though they look close.
write("secure-names.zip", archive([stored(name, hello) for name in [b"a..b", b"..a", b"a/.../b", b"console.txt", b"com10", b"lpt0", b"aux_", b"./x"]]))
# Two entries of one name, and a file and a folder of one name (1491).
write("duplicate.zip", archive([stored(b"same.txt", hello), stored(b"same.txt", b"other\n")]))
write("duplicate-folder.zip", archive([stored(b"a", hello), stored(b"a/", b"")]))
# The name of the local header is not the central one's (1491).
good = archive([stored(b"good.txt", hello)])
write("name-mismatch.zip", good.replace(b"good.txt", b"evil.txt", 1))
# The size written is less than what the data opens into: a bomb that lies (1506).
zeros = bytes(1 << 20)
lie = (b"bomb.bin", 8, raw_deflate(zeros), zlib.crc32(zeros), 1000, 0)
write("size-lie.zip", archive([lie]))
# The size written is more than the data opens into.
write("size-short.zip", archive([(b"short.txt", 8, raw_deflate(hello), zlib.crc32(hello), len(hello) + 5, 0)]))
# A stored entry whose two sizes differ.
body = local(b"stored.txt", 0, zlib.crc32(hello), len(hello), len(hello)) + hello
cd = central(b"stored.txt", 0, zlib.crc32(hello), len(hello), len(hello) - 1, 0)
write("stored-sizes.zip", body + cd + end(1, len(cd), len(body)))
# A wrong CRC.
write("crc.zip", archive([(b"crc.txt", 0, hello, zlib.crc32(hello) ^ 1, len(hello), 0)]))
# Encrypted, and a method not read.
write("encrypted.zip", archive([stored(b"plain.txt", hello), stored(b"secret.txt", hello, flags=1)]))
write("method.zip", archive([stored(b"plain.txt", hello), (b"bzip2.bin", 12, b"BZh9", 0, 4, 0)]))
# Data that is not DEFLATE.
write("corrupt-data.zip", archive([(b"broken.bin", 8, b"\xff\xff\xff\xff", 0, 4, 0)]))
# A DEFLATE stream shorter than the compressed size written, and one longer.
d = raw_deflate(hello)
write("stream-short.zip", archive([(b"pad.txt", 8, d + b"\0\0\0", zlib.crc32(hello), len(hello), 0)]))
write("stream-long.zip", archive([(b"cut.txt", 8, d[:-2], zlib.crc32(hello), len(hello), 0)]))
# Too many entries for the size of the directory (1507).
body = local(b"a", 0, zlib.crc32(hello), len(hello), len(hello)) + hello
cd = central(b"a", 0, zlib.crc32(hello), len(hello), len(hello), 0)
write("count-bomb.zip", body + cd + struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, 0xFFFE, 0xFFFE, len(cd), len(body), 0))
# A directory larger than the file.
write("directory-past-end.zip", body + cd + end(1, 1 << 30, len(body)))
# Every entry at one offset: the plainest overlap.
body = local(b"a", 8, zlib.crc32(zeros), len(raw_deflate(zeros)), len(zeros)) + raw_deflate(zeros)
cd = b"".join(central(n, 8, zlib.crc32(zeros), len(raw_deflate(zeros)), len(zeros), 0) for n in [b"a", b"b", b"c"])
write("overlap-same.zip", body + cd + end(3, len(cd), len(body)))

# David Fifield's overlapping bomb ("A better zip bomb", 2019): the data of each entry quotes the
# header of the next in a stored block that is not final, so every entry ends in one kernel.
def fifield(count, kernel_size):
    kernel_data = bytes(kernel_size)
    kernel = raw_deflate(kernel_data)
    names = [f"{i:04}".encode() for i in range(count)]
    # Built from the last entry back: each entry's data and what it opens into.
    datas = [None] * count
    opened = [None] * count
    headers = [None] * count
    datas[-1] = kernel
    opened[-1] = kernel_data
    headers[-1] = local(names[-1], 8, zlib.crc32(kernel_data), len(kernel), kernel_size)
    for i in range(count - 2, -1, -1):
        h = headers[i + 1]
        quote = struct.pack("<BHH", 0, len(h), len(h) ^ 0xFFFF)
        datas[i] = quote + h + datas[i + 1]
        opened[i] = h + opened[i + 1]
        headers[i] = local(names[i], 8, zlib.crc32(opened[i]), len(datas[i]), len(opened[i]))
    body = headers[0] + datas[0]
    # Offsets: header i sits just after the quote of entry i-1.
    offsets = [0]
    for i in range(1, count):
        offsets.append(offsets[-1] + len(headers[i - 1]) + 5)
    cd = b"".join(central(names[i], 8, zlib.crc32(opened[i]), len(datas[i]), len(opened[i]), offsets[i]) for i in range(count))
    return body + cd + end(count, len(cd), len(body))

# A name that is not UTF-8 is read in code page 437 (1514): 0x81 is "ü".
write("cp437.zip", archive([stored(b"m\x81ller.txt", hello)]))
# A data descriptor whose CRC is not the central directory's, with its signature and without.
for sig, label in [(True, "descriptor-crc.zip"), (False, "descriptor-crc-nosig.zip")]:
    crc = zlib.crc32(hello)
    body = local(b"dd.txt", 0, 0, 0, 0, flags=8) + hello
    body += (struct.pack("<I", 0x08074B50) if sig else b"") + struct.pack("<III", crc ^ 1, len(hello), len(hello))
    cd = central(b"dd.txt", 0, crc, len(hello), len(hello), 0, flags=8)
    write(label, body + cd + end(1, len(cd), len(body)))
# A data descriptor that agrees.
crc = zlib.crc32(hello)
body = local(b"dd.txt", 0, 0, 0, 0, flags=8) + hello + struct.pack("<IIII", 0x08074B50, crc, len(hello), len(hello))
cd = central(b"dd.txt", 0, crc, len(hello), len(hello), 0, flags=8)
write("descriptor.zip", body + cd + end(1, len(cd), len(body)))

bomb = fifield(50, 1 << 20)
write("overlap-fifield.zip", bomb)
zipfile.ZipFile(io.BytesIO(bomb)).testzip()  # Python opens it: 50 MB from a few KB
print("made", len(os.listdir(out)), "archives; bomb", len(bomb), "bytes")
