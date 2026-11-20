# Zip archives for `ZipReader`

`check.sg` reads every archive of the three folders and prints what `ZipReader` finds (choice
1512): each entry with its sizes, method, time and the CRC-32 of its bytes, or the error of the
archive or of the entry. `tests/cli.rs` (`a_zip_suite`) runs it interpreted and compiled and
compares what it prints with `expected.txt`.

- `go/` is copied from Go 1.27.4 (`src/archive/zip/testdata`), under the BSD licence of the Go
  authors kept beside it as `LICENSE`; `go-no-datadesc-sig.zip` is Go's `.zip.base64` decoded.
  `ZipReader` reads them as Go reads them, entry for entry and byte for byte, but for three
  choices: `dupdir.zip` holds a file and a folder both named `a/b`, `DuplicateName` here (1491,
  1513); the time of DOS that makes no day (0 for the date) is 1980-01-01 here, 1979-11-30 in Go
  (1515); and what Go calls a format error is `NotZip` when no end of a central directory is found.
- `made/` is made by `make.py` (`python3 -I make.py made`), most of it wrong on purpose: every name
  that could lead out of a folder (1502) or that another system would read another way (the rule of
  `Path.joinInside`, 1594), two entries of one name, also in another case, NTFS times a `DateTime`
  does not hold and a locator of zip64 past 2^63 (1594), a local name that is not the
  central one, sizes that lie (a bomb that says 1,000 bytes and opens into a megabyte), a wrong
  CRC in the directory and in a data descriptor, an encrypted entry, a method not read, DEFLATE
  data that is broken, shorter or longer than its compressed size, more entries than the
  directory holds, a directory past the end of the file, entries at one offset, and David
  Fifield's overlapping bomb (50 entries of a megabyte each from 5.5 KB, which Python's `zipfile`
  opens). A name in code page 437 and a data descriptor that agrees are read.
- `others/` holds an archive written by Python's `zipfile` (a comment, an entry with zip64's extra
  field, a name in UTF-8, a folder) and one by Info-ZIP's `zip` 3.0 (its fields of Unix time).
  Go reads both as `ZipReader` does.
