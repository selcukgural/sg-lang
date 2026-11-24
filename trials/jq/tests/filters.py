#!/usr/bin/env python3
"""A differential test of a jq of trials/jq (SG, Rust or Zig) against jq 1.8.2: filters on several
inputs, numbers, output styles, input and its errors, bad UTF-8, the language, deep nesting, flags
and files. Written with the Rust version; each of the three passes it.

Each case runs both programs with the same arguments and standard input and compares the bytes on
the standard output and the exit code (the standard error is not compared).

    python3 -I trials/jq/tests/filters.py JQ_BINARY [-k substring] [-v]

jq 1.8.2 is taken from the PATH, or from the environment's JQ. Files are written in
trials/target/jq-tests.
"""

import os
import shutil
import subprocess
import sys

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
# What the cases write and read: files, and the large inputs `json-input` makes.
HERE = os.path.join(REPO, "trials", "target", "jq-tests")
os.makedirs(HERE, exist_ok=True)
JQ = os.environ.get("JQ") or shutil.which("jq") or "jq"
try:
    version = subprocess.run([JQ, "--version"], capture_output=True, text=True).stdout.strip()
except OSError:
    version = ""
if version != "jq-1.8.2":
    sys.exit(f"jq 1.8.2 is wanted (on the PATH, or named by JQ=); found {JQ!r}: {version or 'nothing'}")

args = sys.argv[1:]
verbose = "-v" in args
if verbose:
    args.remove("-v")
only = None
if "-k" in args:
    i = args.index("-k")
    only = args[i + 1]
    del args[i : i + 2]
if len(args) != 1:
    sys.exit(__doc__)
RUST = args[0]

cases = []  # (name, argv, stdin bytes)


def case(argv, stdin=b"", name=None):
    if isinstance(stdin, str):
        stdin = stdin.encode()
    cases.append((name or " ".join(argv) + " <<< " + repr(stdin[:60]), argv, stdin))


DOC = b'''{"a":1,"b":[1,2,{"c":"x"}],"d":{"e":null,"f":true,"g":false},"h":"text","i":1.50,"j":-3,"k":[],"l":{}}'''
SMALL = b'1 "a" null true false [1,[2]] {"a":{"b":1}} -0 2.0 [] {}'
ARR = b'[3,1,2,"b","a",null,true,false,[1],{"a":1},1.0,-1,0.5]'
RECS = b'''[{"id":1,"name":"ada","city":"x","score":5,"tags":["a","b"]},
{"id":2,"name":"bob","city":"y","score":7,"tags":[]},
{"id":3,"name":"cy","city":"x","score":5.0,"tags":["c"]},
{"id":4,"name":"di","city":null,"score":-1,"tags":["a","c","d"]}]'''

# Filters run with -c on several inputs.
FILTERS = r'''
.
..
.a
.b
.b[0]
.b[-1]
.b[5]
.b[-5]
.b[1:]
.b[:2]
.b[-2:]
.b[1:2]
.b[1.5:2.5]
.h[1:3]
.h[-2:]
.d.e.x
.a?
.b[]?
.[]?
.["a"]
."a"
."h"
.a.b?
[.[]?]
[..]
[.. | numbers]
[.. | strings]
[.. | scalars]
[.. | iterables]
[.. | arrays]
[.. | objects]
[.. | booleans]
[.. | nulls]
[.. | values]
keys
keys_unsorted
[paths]
to_entries
length
utf8bytelength?
type
[.[]? | type]
tostring
tojson
@text
@json
@base64
@uri
@csv?
@tsv?
tojson | fromjson
[.[]? | tostring]
[.[]? | tojson]
[.[]? | length?]
map(type)?
map_values(type)?
with_entries(.value |= type)?
to_entries?
[.[]? | numbers | . + 1]
[.[]? | numbers | . * 2]
[.[]? | numbers | . - 1]
[.[]? | numbers | . / 2]
[.[]? | numbers | . % 2]
[.[]? | numbers | -.]
[.[]? | numbers | floor]
[.[]? | numbers | sqrt]
[.[]? | numbers | tostring]
[.[]? | strings | ascii_upcase]
[.[]? | strings | ascii_downcase]
[.[]? | strings | test("e")]
[.[]? | strings | startswith("t")]
[.[]? | strings | ltrimstr("t")]
[.[]? | strings | rtrimstr("t")]
[.[]? | strings | split("x")]
sort?
unique?
min?
max?
add?
any?
all?
reverse?
[.[]?] | sort
[.[]?] | unique
[.[]?] | group_by(type)
[.[]?] | unique_by(type)
[.[]?] | sort_by(type)
[.[]?] | min_by(type)
[.[]?] | max_by(type)
[.[]?] | add?
[.[]?] | length
[.[]?] | first
[.[]?] | last
[.[]?] | reverse
[.[]?] | [first(.[]), last(.[])]
[.[]? | select(. != null)]
[.[]? | select(type == "number")]
[limit(2; .[]?)]
[first(.[]?)]
[last(.[]?)]
isempty(.[]?)
[.[]?] | any
[.[]?] | all
[.[]?] | any(type == "string")
[.[]?] | all(type == "string")
any(.[]?; . == 1)
all(.[]?; . == 1)
[path(..)]
[path(.[]?)]
[paths]
del(.a)?
del(.[0])?
del(.[]?)
to_entries? | from_entries
with_entries(select(.key != "a"))?
[getpath(["a"], ["b", 0], ["x", "y"])?]
setpath(["a"]; 9)?
setpath([0]; 9)?
delpaths([["a"], ["b"]])?
.a = 5
.a |= . + 1
.a += 1
.a -= 1
.a *= 2
.a /= 2
.a %= 2
.a //= 7
.z //= 7
.[]? |= 1
.[]? += 1
(.. | numbers) |= . + 1
(.. | select(type == "string")) |= ascii_upcase
[.. | select(type == "number")] | add
has("a")?
has(0)?
in({"a":1})?
[.[]? | in({"a":1})?]
. as $x | [$x, .]
. as $x | $x
[.[]? as $x | $x]
reduce .[]? as $x (0; . + 1)
reduce .[]? as $x (null; .)
[foreach .[]? as $x (0; . + 1)]
[foreach .[]? as $x (0; . + 1; [$x, .])]
if . then "t" else "f" end
if type == "object" then .a elif type == "array" then .[0] else . end
if . == null then 1 end
[.[]? | if . then 1 else 0 end]
(. == .), (. != .), (. < .), (. <= .), (. > .), (. >= .)
[. == 1, . == "a", . < 1, . > "a", . < [], . > {}]
. and true, . or false, (. | not)
[.[]? | . and true]
[.[]? | . or false]
{a: 1, b: .}
{(tostring): 1}
{"x\(type)": 1}
[{a: (1, 2)}]
[{a: (1,2), b: (3,4)}]
[.[]?] | join(",")?
[.[]? | strings] | join("-")
[.[]? | tostring] | join(", ")
tojson | test("a")
"\(.)"
"x\(.)y\(type)z"
@base64 "v=\(.)"
@json "v=\(.)"
@uri "v=\(.)"
[.[]?] | @csv?
[.[]?] | @tsv?
[.[]? | scalars] | @csv
[.[]? | scalars] | @tsv
try error("x")
[.[]? | try error("x")]
error(null)?
(error("x"))?
[(1, error("x"), 2)?]
[.[]? | (.a)?]
[.[]? | .a?]
[.[]? | .[0]?]
[.[]? | .["a"]?]
.. |= .
[.. | type] | length
first
last
[range(3)]
[range(1; 4)]
[range(0; 10; 3)]
[range(5; 0; -2)]
[range(0; 1; 0.25)]
input?
[inputs]
getpath(["a"])
'''.strip().split("\n")

INPUTS = [DOC, SMALL, ARR, RECS, b'"hello, world"', b"null", b"123", b"-1.5e3", b"[1,2,3]", b'{"a":1.0,"b":"x"}']
for f in FILTERS:
    for inp in INPUTS:
        case(["-c", f], inp)

# Numbers: literals kept and canonicalised, computed doubles.
NUMS = (
    b"[0, -0, 0.0, -0.0, 1, -1, 1.0, 1.10, 2.0, 100, 1e2, 1E2, 1e+2, 1.5e3, 1.0e2, 1e-7, 1e-6, 0.000001, "
    b"0.0000001, 0.00001, 0.0001, 0.0000000, 0e10, 0E-3, 100e-2, 12.3456e1, 1e15, 1e16, 1e17, 1e21, "
    b"123456789012345678901234567890, 18446744073709551617, 18446744073709551616, 9007199254740993, "
    b"0.1, 0.10000000000000000001, 3.141592653589793238462643383279, 1e1000, -1e1000, 1e-400, 1e308, "
    b"1.7976931348623157e308, 1.7976931348623159e308, 5e-324, 2e-324, 3e-324, 4.9e-324, 2.2250738585072014e-308, "
    b"1e999999999, 1e-999999999, 1e1000000000, 1e-1000000000, 1e999999999999, 1e-999999999999, -1e-999999999999, "
    b"12345678901234567890e-10, 0.30000000000000004, 1234567.0, 99999999999999999999]"
)
for f in [".", "map(. + 0)", "map(-.)", "map(. * 1)", "map(tostring)", "map(tojson)", "map(length)", "sort",
          "unique", "map(. == 1)", "map(. < 1)", "group_by(.)", "map(floor)", "map(sqrt)", "@csv", "@tsv",
          "map(. / 3)", "map(. * 1e300)", "map(-(.) * 1)", "min", "max", "add", "map(tonumber)",
          "map(tostring | tonumber)", "[.[] | [.] | add]", "map(. % 7)", "map(-. | -.)",
          "map(. as $x | [$x])", "[.[] | select(. > 1e20)]", "map([.] | tojson | fromjson)"]:
    case(["-c", f], NUMS)
for expr in ["0.1 + 0.2", "1 / 3", "2 / 3", "10 / 3", "1e300 * 1e10", "-1e300 * 1e10", "1e-5 * 1", "0.0001 * 1",
             "1e16 * 1", "1e15 * 1", "123456789012345678 * 1", "1e17 * 1", "1e21 * 1", "1.5e300 * 1",
             "5e-324 * 1", "2e-310 * 1", "3.0 * 1", "-0 * 1", "0 * -1", "-(0 * 1)", "(-1 | sqrt)", "[(-1|sqrt)]",
             "1e1000", "-1e1000", "100000000000000000001 > 100000000000000000000",
             "100000000000000000001 == 100000000000000000000", "1.0 == 1", "1.0", "-1.0", ".5", "1.", "1.50",
             "-1.50", "-(-1.50)", "1e999999999999", "[1.0, 1] | sort", "[1, 1.0] | unique", "1E2", "-0", "-0.0",
             "0.00001", "1e-7", "[.1, 1.e2]", "3 % 2", "5 % -3", "-5 % 3", "5.5 % 2", "5 % 2.5", "1e30 % 7",
             "-1e30 % 7", "[(5 % 0)?]", "[(1 / 0)?]", "0 / 1", "1 - 1", "[(-1|sqrt)] | sort", "[1, (-1|sqrt), 0] | sort",
             "(-1|sqrt) < 1", "(-1|sqrt) == (-1|sqrt)", "(-1|sqrt) < (-1|sqrt)", "[(-1|sqrt)] | unique",
             "1e1000 > 1e999", "1e1000 == 1e1001", "-1e1000 < 1e-1000", "0.1 + 0", "1/3*3", "4 * 0.1",
             "9007199254740993", "9007199254740993 + 0", "[range(1.0; 3)]", "[range(0.5; 3)]",
             "[limit(3; range(0; infinite))]", "1e2 | tostring", "1e2 | tojson", "\"1e2\" | tonumber",
             "\"1.50\" | tonumber", "\" 1\" | tonumber?", "\"nan\" | tonumber", "\"-inf\" | tonumber",
             "\"+1\" | tonumber", "\".5\" | tonumber", "\"1.\" | tonumber", "\"01\" | tonumber", "\"0x1\" | tonumber?",
             "\"1e1000\" | tonumber", "\"Infinity\" | tonumber", "[\"1\", \"2.0\"] | map(tonumber) | add",
             "123456789012345678901234567890 | . + 0", "-123456789012345678901234567890", "[-1e-999999999999]",
             "1e-999999999999 | -.", "1.000000000000000000000000001 == 1", "1.000000000000000000000000001 > 1",
             "[1.000000000000000000000000001, 1] | sort", "[1, 1.000000000000000000000000001] | unique",
             "0.1 == 0.10000000000000000001", "0.1 < 0.10000000000000000001", "-0 == 0", "-0.0 < 0",
             "[0.0, -0, 0] | unique", "1e2 == 100", "[100, 1e2] | unique", "{\"a\": 1e2} | .a", "1e2 | -.",
             "[1e-7, 1e-7 * 1]", "1e-6 * 1", "1.5e-7 * 1", "123e-7 * 1", "0.000123 * 1", "1e-4 * 1",
             "12345678901234567 * 1", "1234567890123456789 * 1", "0.1e-3 * 1", "[.1e1]",
             "1e15 + 0.3", "1e15 + 0.25", "-(0) * 3", "-(0)", "0 | -(.) * 3", "[-(0), -(0.0)] | map(. * 3)"]:
    case(["-nc", expr])

# Output styles.
STYLED = b'{"b":[1,{"z":[],"y":{}},"s\\u0001\\u007f\\u001f\xc3\xa9/"],"a":{"d":2,"c":[3,[4]]},"e":""}'
for opts in [[], ["-c"], ["--tab"], ["--indent", "0"], ["--indent", "1"], ["--indent", "7"], ["--indent", "3", "-c"],
             ["-c", "--indent", "3"], ["--tab", "-c"], ["-c", "--tab"], ["--indent", "3", "--tab"],
             ["--tab", "--indent", "3"], ["--indent", "-1"], ["--indent", "8"], ["--indent", "x"], ["-S"],
             ["-S", "-c"], ["-r"], ["-rc"], ["-cr"], ["-S", "--tab"], ["--sort-keys"], ["--compact-output"],
             ["--raw-output"]]:
    case(opts + ["."], STYLED)
    case(opts + [".b[2], .e, .a, (.b|tojson)"], STYLED)
case(["-r", ".[]"], b'["a\\nb", "c\\u0000d", 1, null, [1], "\\"q\\""]')
case(["-S", "-c", "., tojson, keys_unsorted, tostring"], b'{"b":{"z":1,"y":2},"a":[{"d":1,"c":2}]}')
case(["-c", "."], b'"\\u0000\\u0001\\b\\f\\n\\r\\t\\u000b\\u001f\\u007f\\u0080\\u00ff\\u2028\\ud83d\\ude00/\\\\\\""')

# Input: values one after another, errors, exit codes.
for inp in [b"{}{}[]1 2\"a\"\"b\"", b"12", b"1 2 x 3", b"1 ]", b"1]", b"1}", b"1,", b"1:", b"1[", b"1{", b"true]",
            b"null,", b"\"a\"]", b"[]x", b"[]1", b"1\"a\"", b"truefalse", b"true false", b"[1,]", b"{\"a\":1,}",
            b"[,1]", b"{\"a\"}", b"{1:2}", b"01", b"-01", b"+1", b".5", b"1.", b"1.e5", b"1e", b"-", b"nan", b"NaN",
            b"-nan", b"infinity", b"-Infinity", b"[nan]", b"'a'", b"]", b"}", b"[}", b"\"a\\tb\"", b"\"a\tb\"",
            b"\"a\x00b\"", b"\"a\x1fb\"", b"\"\\ud800\"", b"\"\\ud800\\u0041\"", b"\"\\udc00\"", b"\"\\u0000\"",
            b"\"\\ud83d\\ude00\"", b"{\"a\":1,\"a\":2,\"b\":3,\"a\":4}", b"1\r\n2\r\n", b"\xef\xbb\xbf[1]",
            b"\xef\xbb\xbf", b"\xef\xbb", b"\xef", b"\xef1", b"\xef\xbb1", b"\xef\xbb\xbf\xef\xbb\xbf1", b"1\x002", b"1 \x00 2", b" \xef\xbb\xbf1", b"", b" ", b"\n\n", b"[1,2", b"{\"a\":", b"\"abc",
            b"1 2 [3", b"[1] [2", b"\"a\\/b\"", b"\"\\x\"", b"\"\\u12\"", b"\"\\u123g\"", b"[1 2]", b"tru", b"nul",
            b"1e999", b"[1e-999]", b"-0", b"1 -", b"- 1", b"\x00", b"1\x00", b"[1]\x00"]:
    case(["-c", "."], inp)
    case(["-c", ".+1"], inp)
for argv, inp in [
    (["-c", ".+1"], b"{} 1"), (["-c", ".+1"], b"1 {}"), (["-e", ".+1"], b"1 {}"), (["-e", ".+1"], b"{} 1"),
    (["-e", "empty"], b"1 2"), (["-e", "."], b"1 null"), (["-e", "."], b"null 1"), (["-e", "."], b"1 false"),
    (["-e", "if .==2 then empty else . end"], b"1 2"), (["-e", "if .==2 then null else empty end"], b"2 1"),
    (["-e", "."], b"1 2 x 3"), (["-e", "."], b""), (["-e", "empty"], b""), (["-e", "error"], b"1"),
    (["-e", "error"], b"null"), (["-e", ".[]"], b"[1, null]"), (["-e", ".[]"], b"[null, 1]"),
    (["-e", ".[]"], b"[false]"), (["-e", ".[]"], b"[]"), (["-ne", "1"], b""), (["-ne", "null"], b""),
    (["-ne", "empty"], b""), (["-ne", "error"], b""), (["--exit-status", "."], b"false"),
    (["-n", "[inputs]"], b"1 2 x 3"), (["-n", "first(inputs)"], b"1 2 x 3"), (["-n", "input, input, input"], b"1 2 x 3"),
    (["-c", "[., input]"], b"1 2 3 4"), (["-c", "[., input]"], b"1 2 3"), (["-c", "[., input]"], b"1 x 3"),
    (["-c", "[., input]"], b"1 2 x 3"), (["-n", "input"], b""), (["-n", "input?"], b""), (["-n", "[inputs]"], b""),
    (["-n", "[inputs]"], b"1 2 3"), (["-n", "[limit(2; inputs)]"], b"1 2 3"), (["-n", "first(inputs), input"], b"1 2 3"),
    (["-n", "[., input]"], b"1 2"), (["-s", "."], b"1 2 x 3"), (["-s", "."], b""), (["-s", "."], b"1 2"),
    (["-sc", "."], b"[1] {\"a\":2}"), (["-ns", "."], b"1 2"), (["-nsc", "[., input]"], b"1 2"),
    (["-nsc", "[., input]"], b"1 x"), (["-sc", "[., input?]"], b"1 2"), (["-nc", "[inputs]"], b"1 2 3"),
    (["-c", "[inputs]"], b"1 2 3"), (["-c", "input"], b"1 2 3"), (["-c", "[., input?]"], b"1 2 3"),
    (["-c", "first(inputs)"], b"1 2 3 4"), (["-n", "reduce inputs as $x (0; . + $x)"], b"1 2 3"),
    (["-c", "--arg", "x", "v", "[$x, .]"], b"1"), (["-c", "--argjson", "x", "{\"a\":1.0}", "[$x, .]"], b"1"),
    (["-c", "--argjson", "x", "[1", "."], b"1"), (["-c", "--arg", "x", "1", "--arg", "y", "2", "[$x, $y]"], b"null"),
    (["-nc", "--arg", "x", "a", "--argjson", "y", "2", "{$x, $y}"], b""), (["-c", "$z"], b"1"),
    (["-c", "[.] | .[0] = $x", "--arg", "x", "late"], b"1"),
]:
    case(argv, inp)

# Bad UTF-8 of each kind.
BAD = [b'"\xe4"', b'"\xe4x"', b'"\xe4\xb8"', b'"\xe4\xb8x"', b'"\xc0\x80"', b'"\xc1\xbf"', b'"\xe0\x80\x80"',
       b'"\xe0\x9f\xbf"', b'"\xed\xa0\x80"', b'"\xed\xbf\xbf"', b'"\xf4\x90\x80\x80"', b'"\xf5\x80"',
       b'"\xf8\x88\x80\x80\x80"', b'"\xff"', b'"\xfe"', b'"\x80"', b'"\x80\x80"', b'"\xbf"', b'"\xc3"',
       b'"\xf0\x9f\x98"', b'"\xf0\x9f\x98\x80"', b'"\xf0\x9f\x98x"', b'"a\xf0\x9f"', b'"\xe4\\n"',
       b'"\xe4\\nab"', b'"\xe4\\u0041"', b'"\xe4\\u0041bc"', b'"\xf0\x9f\\n"', b'"\xf0\\u00e9"', b'"\xf0\\ud83d\\ude00"',
       b'"x\xe4"', b'"\xe4\xb8\\u00e4"', b'"\xe4\\""', b'"\xe4\\"xy"', b'"\xe4\\\\"', b'"\\\xe4"', b'\xff',
       b'1\xff', b'[\xff]', b'"a"\xff', b'{"\xe4":1}', b'{"a\xff":"\xff"}', b'["\xe4", "\xc3\xa9", "\xed\xa0\x80"]',
       b'"\xc3\xa9\xe4\xc3\xa9"', b'"\xe4\xe4\xe4"', b'"\xf0\x9f\x98\xf0\x9f\x98\x80"', b'"\xc2"', b'"\xdf\xbf"',
       b'"\xef\xbf\xbf"', b'"\xf4\x8f\xbf\xbf"', b'"ok" "\xe4" "ok"', b'"\xe4']
for b in BAD:
    case(["-c", "., length, utf8bytelength"], b)
    case(["-c", "."], b + b" 1")

# Bad UTF-8 at a piece boundary of 64 KB.
for pad in [65533, 65534, 65535, 65536, 65537]:
    case(["-c", "length"], b'"' + b"a" * (pad - 1) + b'\xe4\xb8\xad\xe4"', name=f"boundary {pad}")
    case(["-c", "length"], b'"' + b"a" * (pad - 1) + b'\xf0\x9f\x98\x80"', name=f"boundary4 {pad}")
    case(["-c", "."], b" " * (pad - 2) + b"123456 789", name=f"number boundary {pad}")
    case(["-c", "."], b" " * (pad - 2) + b"true false", name=f"literal boundary {pad}")

# The language.
LANG = r'''
[(1,2) + (10,20)]
[range(0,1; 3,4)]
[{a:(1,2), b:(3,4)}]
["\(1,2) \(3,4)"]
[[[1,2],[3,4]] | .[][0,1]]
[[0,1,2,3,4,5] | .[(1,2):(3,4)]]
[(1,2) as $x | ($x, 10)]
[limit(-1; 1,2)]
[limit(0; 1,2)]
[limit(1; 1,2)]
[limit(2.5; 1,2,3,4)]
[limit(3; range(0; 10000000))]
[first(range(10; 0; -1))]
[last(range(5))]
[last(empty)]
[first(empty)]
[range(0;10;0)] | length
[range(1.0;3)]
[range(0;1;0.3)]
[1,2,3] | .[] |= empty
[1,2,3,4] | (.[] | select(. >= 2)) |= empty
reduce range(3) as $x (0; empty)
[foreach (1,2,3) as $x (0; if $x == 2 then empty else . + $x end)]
[foreach (1,2,3) as $x (10; if $x == 2 then empty else . + $x end)]
[foreach (1,2) as $x (0; (. + 1, . + 10); [$x, .])]
reduce range(3) as $x (0,10; .+$x)
reduce (1,2) as $x (0; (. + $x, . * 10))
"abc"|reverse
null|reverse
[1,2,3]|reverse
{}|reverse
{"a":1}|reverse
1|ltrimstr("a")
"ab"|ltrimstr(1)
"ab"|ltrimstr("a"), rtrimstr("b"), ltrimstr("x"), ltrimstr(""), ltrimstr("ab")
"aéb" | ltrimstr("a"), rtrimstr("b")
[(1,2) // 3 | if . == 1 then error("x") else . end]
[(1 // 2) | error("x")]?
[.[]? | error("y")]
[(1,2)? | error("z")]
[(1,error("e"),2) // 3]
[(null,false) // 3]
[(null,false,error("q")) // 3]
[empty // 3]
[(1,null,2) // 3]
[error("x") // 1]
[(1|error) // 1]
[.a // 1]
[1 | .a // 2]
[1 | (.a)? // 2]
[(null, 1) // (2, 3)]
[false // false]
path(1)
path(null)
[paths]
{"a":[1,{"b":2}]} | [paths]
{"a":[1,{"b":2}]} | [path(..)]
{"a":[1,{"b":2}]} | [path(.a[1:2])]
{"a":[1,{"b":2}]} | path(.a | map(.))
{"a":[1,{"b":2}]} | [path(.a[] | select(type=="object") | .b)]
{"a":[1,{"b":2}]} | [path(first(.a[]))]
{"a":[1,{"b":2}]} | [path(getpath(["a",1,"b"]))]
{"a":[1,{"b":2}]} | [path(.a | getpath([1,"b"]))]
{"a":1} | [path(if .a then .b else .c end)]
{"a":1} | [path(.a // .b)]
{"a":null} | [path(.a // .b)]
{"a":1} | [path(. as $x | .a)]
{"a":1} | [path(reduce (1,2) as $x (.; .a))]
{"a":1} | [path(1 | .a)]
{"a":1} | [path(.a | 1)]
{"a":null} | [path(.a | null)]
{"a":true} | [path(.a | true)]
{"a":true} | [path(.a | false)]
null | [path(null | .a)]
[1,2] | path(last(.[]))
[1,2] | [path(limit(1; .[]))]
[1,2] | [path(.[]?)]
1 | [path(.[]?)]
1 | [path(..)]
{"a":{"b":1}} | [path(.a.b?)]
[[1,2],[3]] | [path(.[][0])]
[1,[2]] | [path(.[] | arrays | .[0])]
[1,[2]] | [path(.[-1])]
{"a":1} | [path(empty)]
{} | [path(.a.b.c)]
null | [path(.[1:])]
"abc" | [path(.[1:])]
[path(1, .a)]?
{"a":1} | .a = (5,6)
{"a":1} | .a += (5,6)
{"a":1,"b":2} | (.a,.b) |= (.+1, .+10)
{"a":null} | .a //= 3, (.b //= (4,5))
[1,2,3] | .[1:] = ["x"]
[1,2,3] | .[1:] |= map(.*2)
[1,2,3] | .[1:] = "x"
[1,2,3] | .[:-1] |= reverse
[1,2,3] | del(.[0,2])
[1,2,3] | del(.[1:])
[1,2,3] | del(.[-1], .[0])
[1,2,3,4,5] | del(.[1:3], .[0])
{"a":1,"b":2} | del(.a, .c)
{"a":{"b":1,"c":2}} | del(.a.b)
{"a":[1,2,3]} | del(.a[0], .a)
null | del(.a)
1 | del(.a)
[1] | del(.a)
{} | del(.[0])
[1,2,3] | delpaths([[5],[-1]])
[[1,2],[3,4]] | delpaths([[0,1],[1]])
[1,2,3] | to_entries
{"a":1} | with_entries(.value += 1)
[{"key":1,"value":2}] | from_entries
[{"k":"a","v":2}] | from_entries
[{"name":"a","value":2},{"key":null,"Name":"b"}, {"key":false, "name":"c"}] | from_entries
[{"key":"a","value":null},{"key":"b"},{"key":"c","Value":3}] | from_entries
[] | from_entries
[[1,2]] | from_entries
null | .[1] = 5
[] | .[-1] = 5
[1] | .[-1] = 5
[1] | .[3] = 5
{} | .a.b.c = 1
null | .a[2].b = 1
[[1,2],[3,4]] | .[][0] |= .*10
{"a":[1,2]} | .a[] += 1
{"a":[1,2]} | .a[] |= empty
{"a":1,"b":2,"c":3} | (.a, .c) |= empty
{"a":1,"b":2,"c":3} | .[] |= select(. != 2)
[1,2,3,4,5,6] | .[] |= select(. % 2 == 0)
[1,2] | .[1.7]
[1,2,3] | .[-1.5], .[1.2:2.7], .[null:2], .[:-1], .[-0.5]
[1,2,3] | .[1e300], .[-1e300], .[1e300:], .[:-1e300]
"aébc" | .[1:3], .[-2:], length, utf8bytelength
{"a":1} | has("a"), has("b")
[1,2] | has(0), has(2), has(-1), has(1.5)
{"a":1} | has(0)
[1] | has("a")
[3,1,2] | getpath([0]), getpath([5]), getpath(["a"])?
{"a":1} | getpath(["a","b"])
{"a":null} | getpath(["a","b"])
null | getpath(["a",0,"b"])
1 | getpath([])
1 | getpath("a")
null | setpath([]; 1), setpath(["a",1]; 2)
null | setpath([-1]; 1)
{"a":1} | setpath(["a","b"]; 1)
[1,2,3] | delpaths([[0],[1]])
[1,2,3] | delpaths([[]])
[1,2,3] | delpaths([])
[1,2,3] | delpaths(["a"])
{"a":1} | .a |= empty
[1,[2]] | .. |= (if type == "number" then .+1 else . end)
1 as $x | 2 as $y | [$x,$y]
[.[]?] | length
["ab" * 0, "ab" * 0.5, "ab" * 1.5, "ab" * -1, 2 * "ab", "ab" * 3, ("ab" * (-1|sqrt))]
[-0, -0.0, (-0|length), (-1|length), (-1.50|length), (null|length), ("aé"|length), ({}|length)]
true | length
[setpath((["a"],["b"]); (1,2))]
[{} | has("a","b")]
[ [1,2,3] | .[(0,1)] ]
[ "a,b" | split(",", "b") ]
[ (1,2) as $x | (3,4) as $y | [$x,$y] ]
{"a":{"b":1,"c":2}} * {"a":{"b":3},"d":4}
{"a":1} * {"a":{"b":2}}
{"a":{"b":2}} * {"a":1}
[1,2,3,1] - [1]
[1,2,[1]] - [[1], 2]
"a,b" / ","
"a,b" / ""
"" / ","
[{"a":1}] | .[0].a |= empty
[null, 1] | min, max
[1, null] | min, max
[] | min, max
[{"a":1,"b":2},{"a":1,"b":1}] | min_by(.a), max_by(.a)
[3,1,2] | sort_by(-.)
[[1,"b"],[1,"a"],[0,"c"]] | group_by(.[0]), unique_by(.[0])
[{"a":2,"b":1},{"a":1,"b":2},{"a":1,"b":1}] | sort_by(.a), sort_by(.a, .b), sort_by(.b; .a)?
[{"a":2,"b":1},{"a":1,"b":2},{"a":1,"b":1}] | group_by(.a) | map(length)
null | keys
1 | keys
[3,1] | keys
{"b":1,"a":2,"é":3,"Z":4} | keys, keys_unsorted
"abc" | test("c$")
"abc" | test("B"), test("B";"i"), test(["B","i"]), test("b";"x"), test("b";"g"), test("b"; null)
"abc" | test("b";"q")
1 | test("b")
"abc" | test("(")
"abc" | test(1)
"abc" | test(["b"])
"abc" | test([])
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab" | test("(a+)+$")
"a\nb" | test("a.b"), test("a.b"; "s"), test("^b"), test("a$")
[1,[2]] | tostring, tojson, @text, @json
"x" | tostring, tojson, @json, @text
"[1,2" | fromjson
"1 2" | fromjson
"nan" | fromjson
" 1.50 " | fromjson
"" | fromjson
"[1]x" | fromjson
"1]" | fromjson
"{\"a\":1,\"a\":2}" | fromjson
1 | fromjson
"ü/?&=a b~-_.!*'()" | @uri
"hello ü" | @base64, (@base64 | @base64d)
"aGVsbG8" | @base64d
"aGVsbG8=x" | @base64d
"a" | @base64d
"!!!!" | @base64d
"/w==" | @base64d
"/w" | @base64d
"4pyT" | @base64d
"4py" | @base64d
"" | @base64, @base64d, @uri, @csv?
[1,2] | @base64
{"a":1} | @csv
[[1]] | @csv
[{}] | @tsv
"x" | @csv
@base64 "a\(1+1)b\("ü")"
@csv "\([1,"a"])"
@tsv "\(["a\tb"])x"
@json "\("x")"
"\(1;2)"
[.[]?, 1] | add
{"a":1} | add
null | add
[[1],[2]] | add
[{"a":1},{"b":2}] | add
["a","b"] | add
[1, null, 2] | add
[1, "a"] | add
add(1,2)
add(empty)
[1,2] | any, all
[] | any, all
[null, 1] | any, all
[1, {}] | any(. + 1 > 0)
any(1, error; .)
all(false, error; .)
any(empty; .)
[1,2] | in({"a":1})?
"a" | in({"a":1})
0 | in([1])
[1,null,"a",true] | join("-")
[[1]] | join(",")
[] | join(",")
["a"] | join(1)
["a", "b"] | join(1)
[{"a":1}] | join(",")
"aBc ÄÖ" | ascii_downcase, ascii_upcase
1 | ascii_downcase
"a,b,,c," | split(","), split(""), split(",,")
"" | split(","), split("")
"abc" | split("abc"), split("abcd")
1 | split(",")
"a" | startswith("a"), endswith("a"), startswith(""), endswith("b")
"a" | startswith(1)
1 | endswith("a")
def f: def g: 3; g; f
def f(x): x * 2; f(3)
def f($a; b): [$a, b, a]; f(1; 2)
def f($a): $a + 1; [f(1,2)]
def f(a; $b): [a, $b]; [f(1,2; 3,4)]
def fac: if . <= 1 then 1 else . * (. - 1 | fac) end; 10 | fac
def f(g): [g, g]; f(1, 2)
def f: 1; def g: f; def f: 2; g
def f: .+1; def g: f | f; 1 | g
def map(f): "mine"; [1] | map(.)
def select(f): "mine"; 1 | select(true)
def recurse: "mine"; [..]
def empty: 1; [empty]
def f(x): x | x; [2 | f(. * 3)]
def f($x): def g: $x; g; f(5)
def f(g): def h: g; h; [1,2] | f(.[0])
def f: reduce .[] as $x (0; . + $x); [1,2,3] | f
def z(f): if . > 0 then . - 1 | z(f) else f end; 3 | z(. + 100)
"x" as $x | {$x, "y": 2, "a b": 3, @base64 "k": 4}
{"a":1} | {a, "b": .a}
{"a":1} | {"a"}
{"a b":1} | {"a b"}
1 as $x | {$x: 2}?
"k" as $x | {$x: 2}
{(1,2|tostring): 3}
{(1): 2}
{"a": 1 | . + 1}
{a: -1}
{a: - 1 | .}
{if: 1, then: 2, else: 3, end: 4, as: 5, def: 6, reduce: 7, foreach: 8, try: 9, and: 10, or: 11, not: 12}
{"a":{"if":1}} | .a.if, .a."if"
{"a":1} | .["a"], ."a", ."a"?, .a?
{"a":1} | ."\("a")"
[1,[2,3]] | .[1].[0]
{"a":[1]} | .a.[0]
. as $x | [$x]
try error("x")
[try (1, error("x"), 3)]
[1,2] | .[0] as $x | $x
1 | [. as $x | $x, .]
if 1 then 2 end
if null then 2 end
if empty then 1 else 2 end
[if (true, false) then 1 else 2 end]
[if true then (1,2) else 3 end]
if 1 then 2 elif 3 then 4 else 5 end
if false then 2 elif null then 4 end
if false then 2 elif true then 4 end
[.[] | if . then "y" else "n" end]?
[true, false, null, 1] | map(not)
[1 and 2, 1 and null, null and error, false or 1, 1 or error, null or false]
[(true, false) and (true, false)]
[(true, false) or (true, false)]
1 == 1 == 1
1 < 2
[1 < 2, "a" < "b", [] < {}, null < false, false < true, true < 0, 0 < "", "" < [], [1] < [1,0], {"a":1} < {"a":2}, {"a":2} < {"b":1}, {"b":1} < {"a":1,"b":1}]
[{"a":1,"b":2} == {"b":2,"a":1}, [1,2] == [1,2], "a" == "a", 1 == 1.0, null == false]
[[3,"a"],[1,"b"],[3,"a",1]] | sort
[{"b":1},{"a":2},{"a":1,"b":1},{"a":1}] | sort
["b","a","B","é","e",""] | sort
-1 | -.
-"a"
--1
- - 1
-(1,2)
[-(1,2) * (3,4)]
1 + 2 * 3 - 4 / 2 % 3
(1 + 2) * 3
1 - -1
"a" + "b" + null + "c"
null + null
{} + null
[] + [1] + [2,3]
{"a":1} + {"b":2} + {"a":3}
1 + "a"
"a" - "a"
{} - {}
[] * 2
{} / {}
1 % "a"
"abc" | .[0]?
"abc" | .[0]
{"a":1} | .[0]
[1] | .["a"]
null | .[null]?
null | .[null]
[1] | .[null]
[1] | .[[1]]
[1,2,1,2,1] | .[[1,2]], .[[1]], .[[]]
[1,2] | .[{"start":1}]?
{} | .[[]]?
[1,2,3] | .[{"start":1,"end":2}]
"abc" | .[{"start":1,"end":2}]
"abc" | .[1:"x"]
null | .[1:2], .["a"], .[0]
1 | .[]
1 | .[]?
null | .[]
{"a":1,"b":2} | [.[]]
"abc" | .[]
true | .a
[1] | .a?
[..] | length
[1, [2, [3, [4]]]] | [..] | length
[[1,2],[3]] | [.[][]]
[{"a":[1,2]},{"a":[3]}] | [.[].a[]]
[1,2,3] | [.[] | select(. > 1)]
[1,2,3] | map(select(. != 2))
[1,2,3] | map(., .)
[1,2,3] | map(empty)
{"a":1,"b":2} | map(. + 1)
{"a":1,"b":2} | map_values(. + 1)
{"a":1,"b":2} | map_values(empty)
[1,2,3] | map_values(empty)
[1,2,3] | map_values(., .)
[[1,2],[3,4]] | to_entries
{"a":1,"b":2} | to_entries | from_entries
{"a":1,"b":2} | with_entries(.key |= ascii_upcase)
{"a":1,"b":2} | with_entries(select(.value > 1))
{"a":1} | with_entries(.key = 1)
[1,2] | with_entries(.)
[3,1,2] | sort, sort_by(.), unique, min, max, reverse, length, add, any, all
[{"a":3},{"a":1}] | sort_by(.a), min_by(.a), max_by(.a), unique_by(.a), group_by(.a)
[{"a":3},{"a":1},{"a":3}] | unique_by(.a) | length
[] | sort_by(.a), group_by(.a), unique_by(.a), min_by(.a), max_by(.a)
1 | sort
"a" | sort_by(.)
{} | min
[1,2,3] | first, last, first(.[]), last(.[]), [limit(2; .[])], isempty(.[]), isempty(empty)
isempty(1, error("x"))
isempty(error("x"))
[limit(1; 1, error("x"))]
[first(1, error("x"))]
[limit(2; 1, error("x"))]
[last(1, error("x"))]
[.[]?] | first
[range(5)] | .[2:4] = ["a"]
[range(5)] | .[2:4] |= ["a", "b", "c"]
[range(10)] | del(.[2:8], .[0])
{"a":[1,2,3]} | .a[1:] |= map(. * 10)
{"a":[1,2,3]} | .a[:1] += ["x"]
[1,2,3] | (.[1:2], .[0]) |= "x"?
[[1]] | .[0][0:1] = []
{} | .a[1:3] = [1]
null | .[1:3] = ["x"]
"abc" | .[1:2] = "x"
{"a":1} | .a[0] = 1
[1] | .a = 1
1 | .a = 1
{"a":1} | .b.c |= 5
{"a":[]} | .a[2] |= 5
[1,2,3] | .[-1] |= . * 10
[1,2,3] | .[-4] |= 10
[1] | .[1e9] = 1
{"a":1,"b":2} | to_entries | map(.value) | add
reduce range(100000) as $i ([]; . + [$i]) | length
reduce range(100000) as $i (""; . + "x") | length
reduce range(100000) as $i ({}; .["k\($i % 100)"] += 1) | length
[foreach range(5) as $i (null; . + $i)]
[foreach range(5) as $i (0; . + $i; select(. % 2 == 0))]
[foreach (1,2) as $x ((10,20); . + $x)]
[foreach empty as $x (0; .)]
reduce empty as $x (0; .)
reduce (1,2) as $x (empty; . + $x)
[reduce (1,2) as $x ((1,2); . + $x)]
reduce .[]? as $x (0; . + $x)
def f: if . < 100000 then .+1|f else . end; 0|f
def f: if . == 0 then 0 else (. - 1 | f) + 1 end; 10000 | f
def f(n): if n == 0 then 0 else 1 + f(n - 1) end; f(1000)
def f($n): if $n == 0 then 0 else 1 + f($n - 1) end; f(1000)
def ack(m; n): if m == 0 then n + 1 elif n == 0 then ack(m - 1; 1) else ack(m - 1; ack(m; n - 1)) end; ack(2; 3)
[recurse(if . < 3 then . + 1 else empty end)]
{"a":[{"b":1}]} | [recurse] | length
[1,[2]] | [recurse(.[]?)]
[[1,[2]] | getpath(["a"])?]
[[1,[2]] | paths]
{"a":1} | tojson | fromjson
"\u00e9\ud83d\ude00\t" | tojson, length, utf8bytelength
"\(1)\(2)"
"a\("b\("c")d")e"
"\([1,{"a":"x"}])"
"\(null)\(true)\(1.0)"
@text "\(1.0)"
@json "\("x\"y")"
@csv "\(["a,b", 1.0])"
@base64 "\(1)"
@base64d "\("YQ==")"
@uri "\("a b")"
[.[] | tostring]?
ltrimstr("a")?
'''.strip().split("\n")
LANG += ['{a: 1 + 1}', '{a: 1, b: 2}', '{a: 1 // 2}', '{a: 1 as $x | $x}', '{a: 1 and 2}', '{a: .b = 1}', '{a: def f: 1; f}', '{a: 1 | 2, b: 3}', '{a: -1 + 2}', '{a: (1 as $x | $x)}', '{a: [1 as $x | $x]}', '{a: f(1 as $x | $x)}', '{a: try 1}', '{a: 1?}', '"Infinity" | tonumber | [., . == 1e1000, . > 1e1000, -. < -1e1000]', '[1e1000000000, 1e1000, 1e999999999999, infinite, -infinite] | sort, unique, map(tostring), @csv', '[infinite, nan, -infinite] | tojson, sort', '[nan] | sort, (.[0] < .[0])', '.a.b?', '.b[]?', '[.[]?.a?]', '..?', '.a??', '{"a":[1,2]} | .a[]?, .a.[]?, .a[0]?, .a[0:1]?, ."a"?', '1 | .a?, .[0]?, .[]?, .[1:]?', '[1,2] | .a?', '1 | [.[]?] , [..?]', '(.a)?', 'reduce range(10001) as $i (0; [.]) | tojson | length', 'reduce range(10000) as $i (0; [.]) | tojson | length', 'reduce range(10001) as $i ({}; {a: .}) | tojson | length', 'reduce range(10001) as $i (0; [.]) | . == .', 'reduce range(10000) as $i (0; [.]) | . < .', 'reduce range(10001) as $i (0; [.]) | [., .] | sort | length', '[1,2] | .[[3]], .[[]]', '[] | .[[1]]', '[1,2] | .[{"start":1}]?, .[{"end":1}]?, .[{"start":null,"end":1}]', '{} | .[{"start":1,"end":2}]?', '"abcdef" | .[{"start":1,"end":3}]', '[limit(3; range(0; infinite))]', '[limit(3; range(0; nan))]', '-1 | -.']
LANG += ['reduce range(10000) as $i ([]; [.]) | . < .', 'reduce range(10001) as $i ([]; [.]) | . < .', 'reduce range(10000) as $i ({}; [.]) | . < .', 'reduce range(10000) as $i ({}; {a:.}) | . < .', 'reduce range(9999) as $i ({}; {a:.}) | . < .', 'reduce range(9999) as $i ({"b":1}; {a:.}) | . < .', 'reduce range(9999) as $i ({"b":1}; [.]) | [., .] | sort | length', 'reduce range(9999) as $i ({}; [.]) | [., .] | sort | length', 'reduce range(10000) as $i ({}; [.]) | [., .] | sort | length', 'reduce range(9999) as $i ({"b":1}; [.]) | [., .] | unique | length', 'reduce range(9999) as $i ({"b":1}; [.]) | [., .] | group_by(.) | length', 'reduce range(10000) as $i ({"b":1}; [.]) | . == [.[0]]', 'reduce range(10001) as $i (1; [.]) | . == [.[0]]', 'reduce range(10001) as $i (1; [.]) | [.] == [[.[0]]]', 'reduce range(10001) as $i (1; [.]) | . != (tojson | fromjson)?', '"abc\\n" | test("c$"), test("c$"; "x"), test("c\\\\Z")?', '"abc\\nx" | test("c$")', '"ab\\n\\n" | test("b$")', '"a$" | test("\\\\$"), test("[$]"), test("a[$]")', '"x\\n" | test("^x$"), test("^x$"; "x")', '"a\\nb" | test("^b"), test("a$"), test("a.b"; "p"), test("a.b")', '"aXb" | test("x"; "i"), test("a x b"; "x"), test("a x b # c"; "x")', '"" | test(""), test(""; "n"), test("a*"; "n")', '"b" | test("a*"; "n"), test("a*")', '[.[]?][1:3]', '[1,2,3] | .[1:] // 5, (.[5:] // 5), (null // .[0])', '[1,2,3] | .[0] as $x | . + [$x]', '[1,2,3] | (.[0] | . + 1) as $x | $x * 2', '{"a":{"b":[1,2]}} | .a | .b | .[1:]', '[[1,2],[3]] | map(.[0] // 0)', '[{"a":1},{"b":2}] | map(.a // "none")', '[{"a":1},{"b":2}] | map(.a as $x | $x)', '[3,1,2] | sort_by(.), sort_by(-.), group_by(. % 2), min_by(-.), max_by(-.), unique_by(. % 2)', '[{"a":[2]},{"a":[1]}] | sort_by(.a[0]), sort_by(.a | .[0] // 0)', '[1,2] | map(error("x") // 1)?', '[1, null] | map(.a // 2)?', '{"a":[1,2,3]} | .a = 5, (.a[1:] = ["x"]), (.a[] = 0), ((.a, .b) = 1)', '{"a":[1,2,3]} | .a[] |= . * 2, (.a[] += 1), (.a |= length), (.a[1:] |= map(-.))', '[1,2,3] | (.[] | select(. > 1)) = 0', '[1,[2,3]] | (.. | numbers) = 0', '{} | .a.b = (1,2)', 'null | (.a, .b) |= 1', '[1,2,3] | (.[0], .[0]) |= . + 1', '{"a":5} | (.a, .a.b) |= empty', '[[1,2],[3,4]] | .[][1] |= empty', '[1,2,3] | .[1] |= error("e")', '[1,2,3] | .[5] = 1, (.[-1] = 9)', 'reduce range(5) as $i ([]; . + [$i * 2])', 'reduce range(5) as $i (0; . + $i) as $s | $s', 'reduce (1, 2, 3) as $x (null; . // $x)', 'reduce empty as $x (1; . + 1)', 'reduce range(3) as $i (0; error("r"))', '[foreach range(5) as $i (0; . + $i)]']
LANG += [r'{"$serde_json::private::Number": "1"}', r'[{"a":1},{"a":"x"}] | map(.a + 1)?', r'[1,2] | map(.a)', r'reduce (1,2) as $x (0; . + $x)',
         r'[.[]?] | map(. * 2)?', r'{"a":[1,2]} | [.a[] | . * 10]', r'[1,[2]] | map(.[0]?)', r'"\(1,2)"', r'[{a:(1,2)}]', r'{(.a // "k"): 1}?',
         r'[range(3) | {a: ., b: "x\(.)"}]', r'reduce range(5) as $i (null; . + $i)', r'reduce .[]? as $x (0; . + 1)', r'[.[]?] | reduce .[] as $x ({}; .[$x|tostring] = $x)?',
         r'[limit(2; .[]?)] | map(tostring)', r'if . then "a" else "b" end | ascii_upcase', r'[1, "a"] | map(-.)', r'{} | .a.b.c', r'[null] | map(.a)']
for f in LANG:
    case(["-nc", f])
    case(["-c", f], b'[1,"a",null,{"a":1}]')

for f in ["reduce range(1000000) as $i (0; . + $i)", "[range(1000000)] | length",
          "def f: if . < 10000000 then .+1|f else . end; 0|f",
          "def f: if . == 0 then 0 else (. - 1 | f) + 1 end; 1000000 | f",
          "[limit(3; range(0; infinite))]", "first(range(0; 1e18))", "[limit(5; range(0; 1e18))]",
          "reduce range(0; 300000) as $i ({}; .[$i | tostring] = $i) | length",
          "[range(0; 1000000)] | sort | length", "[range(300000) | tostring] | join(\",\") | length",
          "[range(100000)] | map(. * 2) | add",
          "[range(100000)] | to_entries | from_entries? | length",
          "[range(100000) | {a: ., b: (. % 10)}] | group_by(.b) | map(length)",
          "def f: if . == 0 then [] else [(. - 1 | f)] end; 10000 | f | tojson | length",
          "reduce range(10001) as $i (0; [.]) | tojson | length",
          "[reduce range(10001) as $i (0; [.])] == [reduce range(10001) as $i (0; [.])]",
          "reduce range(1000000) as $i (0; [.]) | length",
          "[range(1000000) | {a: ., b: \"x\\(.)\"}] | map(.a + 1) | add",
          "reduce range(1000000) as $i ([]; . + [$i]) | length",
          "[range(10) as $x | reduce range(100000) as $i ({}; .[\"k\\($i % 7)\"] += $x)] | length"]:
    case(["-nc", f], name="heavy " + f)

# Deep nesting.
for depth in [9999, 10000, 10001, 20000]:
    case(["-c", "tojson | length"], b"[" * depth + b"]" * depth, name=f"deep array {depth}")
    case(["-c", "."], b"[" * depth + b"]" * depth, name=f"deep array print {depth}")
for depth in [4999, 5000, 5001]:
    case(["-c", "tojson | length"], b'{"a":' * depth + b"1" + b"}" * depth, name=f"deep object {depth}")
for inner in [b'{"a":1}', b"{}", b'{"a":[]}', b"[1]", b'"x"']:
    for depth in [9998, 9999]:
        case(["-c", "tojson | length"], b"[" * depth + inner + b"]" * depth, name=f"deep {depth} {inner!r}")
case(["."], b"[" * 10000 + b"]" * 10000, name="deep pretty 10000")
case(["-c", "[..] | length"], b"[" * 10000 + b"]" * 10000, name="deep recurse 10000")
case(["-c", "[paths] | length"], b"[" * 3000 + b"]" * 3000, name="deep paths 3000")
case(["-c", ". == ."], b"[" * 10000 + b"]" * 10000, name="deep compare 10000")
case(["-c", "[.] | sort | length"], b"[" * 9999 + b"]" * 9999, name="deep sort 9999")
case(["-c", "."], b"[" * 100000, name="deep unfinished 100000")
case(["-c", "."], b"[" * 300000 + b"]" * 300000, name="deep 300000")
case(["-c", "."], b'{"a":' * 200000 + b"1" + b"}" * 200000, name="deep object 200000")

for lit in [b"[15e-1147483647, 25e-1147483647, 35e-1147483647, 5e-1147483647, 4e-1147483647, -25e-1147483647, 125e-1147483648, 2, 1.9999999999999999999]"]:
    case(["-c", "., (.[-2] > .[-1])"], lit)
case([".", "-c"], b"[1, 2]", name="option after filter")
case([".", "-", "-c"], b"[1]", name="dash file")
case(["-c", "--arg", "x", "1", ".", "--argjson", "y", "2"], b"[1]", name="options around")

# Flags and files.
case(["-cn", "1, 2"])
case(["-nr", '"a", 1'])
case(["-Sc", '{"b":1,"a":{"d":1,"c":2}}'], b"null")
case(["-c"], b"1", name="no filter")
case(["-x", "."], b"1", name="unknown flag")
case(["--frobnicate", "."], b"1", name="unknown long flag")
case(["-c", "{"], b"1", name="syntax error")
case(["-c", "foo"], b"1", name="unknown function")
case(["-c", "foo(1)"], b"1", name="unknown function/1")
case(["-c", "1 +"], b"1", name="unfinished")
case(["-c", "(1"], b"1", name="unbalanced")
case(["-c", "\"\\(1\""], b"1", name="unfinished interpolation")
case(["-c", ". # a comment\n| . + 1 # another"], b"1")
case(["-c", "#only a comment"], b"1")
case(["-c", ""], b"1")
case(["-c", "def f: 1;"], b"1")


# Differences that are meant: jq's quirks the spec says not to follow (input refused with exit 5),
# a NUL byte inside a number token (jq reads the token as a C string and stops at the NUL), and
# Oniguruma's retry limit, which makes jq fail on "(a+)+$" where Rust's regex answers false.
EXPECTED_INPUTS = [b"01", b"-01", b"+1", b".5", b"1.", b"1.e5", b"nan", b"NaN", b"-nan", b"infinity",
                   b"-Infinity", b"[nan]", b'"\\udc00"', b"1\x002", b"1\x00"]


def is_expected(name, stdin):
    return stdin in EXPECTED_INPUTS or "(a+)+$" in name or '"nan" | fromjson' in name


def run(argv, stdin, cwd):
    try:
        p = subprocess.run(argv, input=stdin, capture_output=True, timeout=60, cwd=cwd)
        return p.stdout, p.returncode
    except subprocess.TimeoutExpired:
        return b"<TIMEOUT>", -999


def main():
    os.makedirs(os.path.join(HERE, "files"), exist_ok=True)
    files = os.path.join(HERE, "files")
    for name, data in {"a.json": b'{"a":1}', "b.json": b"[2", "c.json": b"]", "bom.json": b"\xef\xbb\xbf3",
                       "n1": b"1", "n2": b"2", "bad.json": b"1 x", "empty.json": b"", "u1": b'"\xe4', "u2": b'\xb8\xad"',
                       "s1": b'"ab', "s2": b'c"'}.items():
        with open(os.path.join(files, name), "wb") as f:
            f.write(data)
    for argv in [["-c", ".", "a.json", "missing.json", "a.json"], ["-c", ".", "b.json", "c.json"],
                 ["-c", ".", "bom.json", "bom.json"], ["-c", ".", "a.json", "bom.json"], ["-c", ".", "n1", "n2"],
                 ["-c", ".", "missing.json"], ["-e", "-c", ".", "missing.json", "a.json"], [".+1", "missing.json", "a.json"],
                 ["-c", ".", "bad.json", "a.json"], ["-c", ".", "empty.json"], ["-c", ".", "empty.json", "n1"],
                 ["-c", ".", "u1", "u2"], ["-c", ".", "s1", "s2"], ["-sc", ".", "n1", "n2", "a.json"],
                 ["-nc", "[inputs]", "n1", "missing.json", "n2"], ["-c", ".", "files"], ["-c", ".", "."]]:
        cases.append(("files " + " ".join(argv), argv, b""))

    failures = 0
    meant = 0
    count = 0
    for name, argv, stdin in cases:
        if only and only not in name:
            continue
        count += 1
        expected = run([JQ] + argv, stdin, files)
        got = run([RUST] + argv, stdin, files)
        if expected != got and is_expected(name, stdin):
            meant += 1
            if verbose:
                print(f"meant {name[:200]}: jq exit {expected[1]}, rust exit {got[1]}")
        elif expected != got:
            failures += 1
            print(f"FAIL {name[:200]}")
            print(f"   jq:   exit {expected[1]} {expected[0][:300]!r}")
            print(f"   rust: exit {got[1]} {got[0][:300]!r}")
        elif verbose:
            print(f"ok   {name[:200]}")
    print(f"{count - failures - meant} of {count} cases agree, {meant} differ as meant, {failures} fail")
    return 1 if failures else 0


sys.exit(main())
