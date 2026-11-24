#!/usr/bin/env python3
"""A differential test of a jq of trials/jq (SG, Rust or Zig) against jq 1.8.2: documents under many
filters, numbers and doubles written as jq writes them, input with whitespace, marks, errors and bad
UTF-8, nesting, recursion, paths, regular expressions, arguments, the language, ending early, files,
and large inputs. Written with the Zig version; each of the three passes it.

Every case runs both programs with the same arguments and standard input and compares the bytes
of the standard output and the exit code (the standard error is not compared).

    python3 -I trials/jq/tests/streams.py JQ_BINARY [-k substring] [-v] [--small]

jq 1.8.2 is taken from the PATH, or from the environment's JQ. Files are written in
trials/target/jq-tests. The large inputs (records-2.json, lines-2.ndjson, numbers-2.json) are made
there by `json-input` (trials/inputs/json.sg), built with interpreter/target/release/sg; when that
is not built, the cases using them are left out and said to be.
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
if not args or args[0].startswith('-'):
    sys.exit(__doc__)
OURS = args.pop(0)
FILTER = None
VERBOSE = '-v' in args
if '-k' in args:
    FILTER = args[args.index('-k') + 1]
SMALL = '--small' in args  # leaves out the cases that take long (for a debug build)

cases = []


def case(argv, stdin=b'', name=None, timeout=60, shell=None):
    if isinstance(stdin, str):
        stdin = stdin.encode()
    cases.append((name or ' '.join(argv), argv, stdin, timeout, shell))


DOCS = [
    '{"a":1,"b":[1,2,3],"c":{"d":"x","e":null},"f":"hello","g":true,"h":1.50}',
    '[3,1,2,null,"b","a",{"x":1},[0],true,false,1.0,-0]',
    '[{"name":"a","n":2,"t":["x","y"]},{"name":"b","n":1,"t":[]},{"name":"a","n":3}]',
    '"Hello, wörld! ✓ a,b"',
    'null',
    '123.4500',
    '{"k":{"a":1,"b":2},"arr":[[1,2],[3,4]],"s":"a,b,c","n":-5}',
    '[]',
    '{}',
    '[1,[2,[3,[4]]],{"a":[5,{"b":6}]}]',
    'true',
    '"x"',
    '[1,2,3,4,5,6,7,8,9,10]',
    '{"a":{"b":{"c":1}},"x":[{"y":1},{"y":2}]}',
]

FILTERS = r'''
.
.a
.a.b
.b[0]
.b[-1]
.b[5]
.b[1:]
.b[:2]
.b[-2:]
.[0]
.[1:3]
.[2:1]
.[-1:]
."a"
.["a"]
.c.d
.c["d"]
.[]
.[]?
.a?
.[0]?
..
[..]
[.. | numbers]
.a,.b
.a//.b
.x // "default"
(.a,.c) // 9
[.[]|select(. != null)]
[.[]?|numbers]
map(.)?
keys
keys_unsorted
keys?
length
length?
utf8bytelength?
type
[.[]?|type]
to_entries?
to_entries|from_entries?
with_entries(.value |= tostring)?
add
add?
any?
all?
[paths]
[paths]?
[path(..)]
path(.a)?
[path(.a[0].b?)]
[path(.[]?)]
[getpath(["a","b"])?]
getpath(["c","d"])?
setpath(["a"]; 9)?
setpath(["z",0]; 1)?
del(.a)?
del(.[0])?
del(.[1,2])?
del(.[]?)
del(..|select(. == 1))?
to_entries?
tostring
tojson
tojson|fromjson
[.[]?|tostring]
[.[]?|tojson]
sort?
sort_by(.n)?
sort_by(type)?
group_by(.name)?
group_by(type)?
unique?
unique_by(.name)?
unique_by(type)?
min?
max?
min_by(.n)?
max_by(.n)?
reverse?
[.[]?|numbers|floor]
[.[]?|numbers|sqrt]
[.[]?|strings]
[.[]?|arrays]
[.[]?|objects]
[.[]?|iterables]
[.[]?|scalars]
[.[]?|booleans]
[.[]?|nulls]
[.[]?|values]
[recurse]
[recurse(.[]?)]
[recurse(.[]?; . != 3)]?
first?
last?
first(.[]?)
[limit(2; .[]?)]
[limit(0; .[]?)]
[first(range(10))]
[range(5)]
[range(2;5)]
[range(0;10;3)]
[range(5;0;-2)]
[.[]?|tostring|ascii_downcase]
ascii_upcase?
ascii_downcase?
split(",")?
split("")?
join(",")?
join("-")?
test("o")?
test("O";"i")?
test("^h")?
startswith("H")?
endswith("b")?
ltrimstr("He")?
rtrimstr("a,b")?
@text
@json
@csv?
@tsv?
@base64
@base64|@base64d
@uri
@html?
"v: \(.)"
"\(.a) and \(.b)"?
@base64 "x\(.)y"
@json "j=\(.)"
@csv "\(.)"?
@tsv "\(.)"?
@uri "u=\(.)"
{a: .a, "b": .b}?
{a, b}?
{(.f // "k"): 1}?
{"x\(1+1)": 2}
{a: (1,2), b: (3,4)}
[{a: (1,2)} | .a]
{$__loc__}?
.a = 5
.a |= .+1?
.b[] |= .*2?
.b[1:] = ["z"]?
.c.d += "!"?
.a -= 1?
.a *= 3?
.a /= 2?
.a %= 2?
.x //= 7?
.a //= 7?
.[]? += 1
map_values(.)?
map_values(empty)?
.[] |= empty?
(.a, .b) = 1?
.. |= (numbers |= .+1)?
[.[]?|numbers] | add
reduce .[]? as $x (0; . + 1)
reduce range(5) as $x (null; . + $x)
foreach .[]? as $x (0; .+1)
[foreach range(5) as $x (0; .+$x; [$x, .])]
[foreach range(5) as $x (0; .+$x; select(. > 3))]
. as $x | [$x, $x]
. as $x | $x | type
def f: . ; f
def f(g): [g, g]; f(1, 2)
def f($a; $b): $a + $b; f(1; 2)
def f(a; $b): a + $b; f(.; 2)?
def fac: if . <= 1 then 1 else . * (. - 1 | fac) end; 10 | fac
def r(n): if n > 0 then n, r(n - 1) else empty end; [r(3)]
[.[]?] | length
if . then "t" else "f" end
if type == "object" then 1 elif type == "array" then 2 else 3 end
if . == null then 1 end
[if (true, false) then 1 else 2 end]
.a and .b
.a or .x
(true, false) and (true, false)
[(true, false) or (true, false)]
not
[.[]?|not]
1 + 2 * 3 - 4 / 2 % 3
-1
-.a?
-(.a)?
[.[]? | -.]?
1, 2 | . * 10
(1,2) as $x | $x * 10
[.[]? | . as $v | $v]
"é😀\t\"\\"
"a" * 3
"abc" / "b"
[1,2,3] - [2]
{"a":1} + {"b":2}
{"a":{"b":1}} * {"a":{"c":2}}
[1,2] + [3]
null + 1
1 + null
"x" + null
[] | add
{} | add
1 == 1.0
1 < "a"
[1,2] < [1,3]
{"a":1} < {"a":2}
{"a":2} < {"b":1}
[3,1,2] | sort
[{"a":2},{"a":1,"b":0}] | sort
0.1 + 0.2
1e1000
-1e1000
1e-400
100000000000000000001 > 100000000000000000000
100000000000000000001 == 100000000000000000000
3.0
1.10
-0
[0.00001, 0.0001, 1e15, 1e16, 1e17, 12345678901234567890, 1.5e300, 5e-324, 2e-310]
[1/3, 2/3, 10/3, 1e15+0.3, 1e16+1, -1/3, 100/7]
[.[]?|length]
[limit(3; repeat(1))]?
input?
[inputs]
$ENV.PATH?
@sh?
try error("x")
error("x")?
[.[]? | error?]
error(null)?
[.[]?|tostring|test("a")]
.a.b.c?
.[1.5]?
[.[]?|.[1.5:2.5]?]
"abcdef" | .[2:4], .[:-1], .[10:], .[-3:-1]
[1,2,3] | .[1:] = [9]
[1,2,3] | .[-1] = 9
[1,2,3] | .[5] = 9
null | .[2] = 1
null | .a.b = 1
{} | .a[1] = 1
[1,2,3] | del(.[0], .[2])
[1,2,3] | del(.[1:])
[1,2,3] | to_entries
{"a":1} | has("a"), has("b")
[1,2] | has(0), has(2), has(-1)
"a" | in({"a":1})
1 | in([0,1])
[3,1,2] | min, max, (sort | first), (sort | last)
[1,[2]] | flatten?
[{"k":"a","v":1}] | from_entries?
$__prog_args?
splits("a")?
[.[]? | @text]
[.[]? | @json]
@base64d?
"YWJj" | @base64d
"4pyT" | @base64d
"/w==" | @base64d
"YQ" | @base64d
@uri "\(.)"
[1,2] | join(",")
["a",1,null,true] | join("/")
[] | join(",")
"a,b,c" | split(",")
"" | split(",")
"abc" | split("")
"aXbXc" | split("X")
tojson | tojson
tojson | fromjson | tojson
[.[]?] | tojson
"[1,2" | fromjson?
"{\"a\":1,\"a\":2}" | fromjson
"1e1000" | fromjson
"100000000000000000001" | fromjson
"1.000" | tonumber
"abc" | tonumber?
1.000 | tostring
[1.000, 1.0e2, -0.0] | tojson
{} | .a.b.c
[[1,2],[3]] | .[][0]
[[1,2],[3]] | [.[][]]
[[1,2],[3]] | map(add)
[[1,2],[3]] | map(length) | add
{"a":[1,2]} | [.a[] | . + 1]
{"a":[1,2]} | .a | map(. * 2)
[1,2,3] | map(select(. > 1))
[1,2,3] | any(. > 2), all(. > 0), any(.[]; . == 2), all(.[]; . < 2)
[] | any, all
[1,2,3] | first(.[]), last(.[])?, first, last
[1,2,3] | [limit(2; .[])]
[range(0;10)] | .[2:4]
[range(3)] | map(tostring) | join("")
{"a":1,"b":2} | with_entries(select(.value > 1))
{"a":1,"b":2} | to_entries | map(.key)
{"b":2,"a":1} | keys, keys_unsorted
{"a":[{"b":1},{"b":2}]} | [paths]
{"a":[{"b":1},{"b":2}]} | [paths | join(".")?]
{"a":[{"b":1},{"b":2}]} | getpath(["a",1,"b"])
{"a":[{"b":1},{"b":2}]} | [leaf_paths]?
{"a":[{"b":1},{"b":2}]} | del(.a[] | select(.b == 1))
{"a":[{"b":1},{"b":2}]} | .a[].b |= . * 10
{"a":[{"b":1},{"b":2}]} | .a |= map(.b)
{"a":[{"b":1},{"b":2}]} | path(.a[].b)
{"a":[{"b":1},{"b":2}]} | [path(..)] | length
'''.strip().split('\n')

for d in DOCS:
    for f in FILTERS:
        case(['-c', f], d + '\n', name=f'-c {f!r} <<< {d[:30]}')

# Output formats.
for d in DOCS + ['{"b":{"d":1,"c":[1,{"z":0,"y":[]}]},"a":"é\\u0000\\u001f\\u007f/"}', '"line\\nbreak\\ttab\\"q\\\\"']:
    for flags in [[], ['-c'], ['-r'], ['-S'], ['--tab'], ['--indent', '0'], ['--indent', '1'], ['--indent', '7'],
                  ['--indent', '3', '-S'], ['-cr'], ['-rS'], ['-c', '--indent', '2'], ['--indent', '2', '-c'],
                  ['--tab', '-c'], ['-c', '--tab'], ['--indent', '-1'], ['--indent', '8'], ['-j'] and [],
                  ['--tab', '--indent', '3'], ['--indent', '3', '--tab']]:
        case(flags + ['.'], d + '\n')
        case(flags + ['.[]?'], d + '\n')

# Numbers: literals kept and made canonical, computed doubles.
NUMBERS = ('1.5e3 1.0e2 1e-7 0.00001 2.0 1.10 -0 0e10 100e-2 1E2 1e1000 1e-400 1e999999999999 '
           '18446744073709551617 123456789012345678901234567890 0.000001 0.0000001 1e-6 1E+0 1e0 10e-1 '
           '-1.5E-10 0.1 100 1000000 1e+2 1E-2 12.3450e-2 0.0 -0.0 0e-5 1e-999999999999 '
           '1e999999999 1e1000000000 15e-1147483647 1e-1147483646 '
           + '1' * 100 + ' 0.' + '0' * 20 + '1 ' + '9' * 40 + 'e-50')
case(['.'], NUMBERS)
case(['-c', '[.]'], NUMBERS)
case(['-c', '. + 0'], NUMBERS)
case(['-c', '-.'], NUMBERS)
case(['-c', 'length'], NUMBERS)
case(['-c', 'tostring'], NUMBERS)
case(['-c', 'tojson'], NUMBERS)
case(['-c', '[., 1] | sort'], NUMBERS)
case(['-c', '. == 1'], NUMBERS)
case(['-c', 'floor'], NUMBERS)
case(['-c', '[.] | @csv'], NUMBERS)
case(['-n', '-c', '[1e1000, -1e1000, 1e-400, 18446744073709551617, 0.1 + 0.2, 1e1000 - 1e1000, 3 - 3, -(3 - 3), 1/3, 2e-310*1, 5e-324 + 0, 1e300*1e300, 4.35, 0.1*3, 1e21+0, 1e20+0, 123456789012345680000+0, 1e-5+0, 1e-4+0, 0.001+0, 1.7976931348623157e308+0, 9007199254740993+0, 2.5e-8 * 1]'])
case(['-n', '-c', '[100000000000000000001, 100000000000000000000] | sort, (.[0] > .[1]), (.[0] == .[1]), unique'])
case(['-n', '-c', '[1.0, 1, 1.00] | unique, sort, group_by(.)'])
case(['-n', '-c', '[1.10, 1.1] | .[0] == .[1]'])
case(['-n', '-c', '[-0, 0, -0.0] | map(-.)'])
for expr in ['5 % 3', '-5 % 3', '5 % -3', '5.5 % 2', '1e20 % 7', '5 % 0.5', '5 % 0', '1 / 0', '0 / 0', '"a" * 0', '"a" * 2', '"ab" * 1.5', '"a" * -1', '2 * "ab"', '[] - 1', '{} * []', '"a" - "a"', 'null * null', '{} / {}']:
    case(['-n', '-c', expr])

# Input: values one after another, whitespace, BOM, CRLF, errors mid-stream, exit codes.
for data in ['{}{}[]1 2"a""b"', '12', '1 2 [3,', '1x', '1 tru', '[1,]', '{"a":1,}', '[1 2]', '1 ]', '1"a"',
             '"a"x', '[1]x', 'truex', '1-2', 'nullnull', '[1,2]3"x"{"a":1}null', '{"a":1,"a":2,"b":3,"a":4}',
             '-', '1e', '"\\x"', '"\\u12"', '{1:2}', '{"a"}', '"\\/"', '[1,2', '﻿[1]', ' ﻿1',
             '1﻿', '[1]\r\n[2]\r\n', '"\\uD800"', '"\\uD800x"', '"\\ud83d\\ude00"', '"\\u0000\\u007f\\u001f"',
             '"\\udc00"', '01', '+1', '.5', '1.', 'nan', 'NaN', '-nan', 'infinity', '-infinity', '[nan]', '',
             '   ', '\n\n', '{"a":[1,{"b":2}]}  [3]', '1,2', ':', ']', '}', '{"a" 1}', '[1:2]', 'tru', 'nul',
             'falsefalse', '"a\\u0041\\n"', '1.5e', '1e+', '-01', '00', '"', '"abc', '[', '{', '{"a":', '[1,{]}]',
             '"tab\there"', '"\x7f"', '[1]]', '[[1]', '{"a":1}}', '"a" "b" "c"', '1 2 3 \n 4']:
    case(['-c', '.'], data)
    case(['-c', '.+1'], data)
    case(['-e', '-c', '.'], data)
for data in ['{} 1', '1 {}', '1 null', 'null 1', 'false', '1 2', '', 'null', '1 [', '{} {}']:
    for flags in [['-c', '.+1'], ['-e', '.+1'], ['-e', '.'], ['-e', 'empty'], ['-e', 'empty, .'],
                  ['-e', '., empty'], ['-c', 'error'], ['-e', 'error'], ['-s', '-c', '.'], ['-n', '-c', '[inputs]'],
                  ['-n', '-c', 'input'], ['-sn', '-c', '., input'], ['-n', '-c', 'first(inputs)'],
                  ['-n', '-c', '[limit(1; inputs)]'], ['-c', '[., input]'], ['-n', '-c', '[.[]?, (input|.+1)?]'],
                  ['-e', '-n', 'empty'], ['-e', '-n', 'null'], ['-e', '-n', '1'], ['-r', '.'], ['-c', '.[]?']]:
        case(flags, data)

# Bad UTF-8 of each kind, inside texts and outside them.
BAD = [b'"a\x80b"', b'"\xc3"', b'"\xc3x"', b'"\xe2\x82"', b'"\xe2\x82x"', b'"\xc0\x80"', b'"\xc1\xbf"',
       b'"\xed\xa0\x80"', b'"\xed\xbf\xbf"', b'"\xf4\x90\x80\x80"', b'"\xf5"', b'"\xff\xfe"', b'"\xe0\x80\x80"',
       b'"\xf0\x80\x80\x80"', b'"\xf0\x9f\x98"', b'"\xf0\x9f\x98\x80"', b'"\xf4\x8f\xbf\xbf"', b'"\xe2\x28\xa1"',
       b'"\xf0\x28\x8c\xbc"', b'"\xf8\x88\x80\x80\x80"', b'"\xc3\xa9\xe9"', b'\x80', b'{"\x80":1}', b'["\xe2\x82",1]',
       b'"\xe2\x82\\n"', b'"\xe2\\u0082"', b'1\x80', b'"ok" \xff', b'"\xc3' + b'"', b'"x\xf0\x9f"',
       b'"\xef\xbb\xbf"', b'\xef\xbb\xbf"bom"', b'"' + b'\xe2\x82\xac' * 30000 + b'\xe2"']
for b in BAD:
    case(['-c', '.'], b)
    case(['-c', '[., length, utf8bytelength]?'], b)
# A bad sequence cut across the 64 KB pieces of the input.
for at in range(65530, 65540):
    case(['-c', 'length'], b'"' + b'a' * at + b'\xe2\x82\xac\xe2\x82' + b'a' * 10 + b'\xf0\x9f\x98\x80' + b'"', name=f'piece edge {at}')
case(['-c', 'length'], b'"' + b'a' * 65534 + b'\xf0\x9f\x98\x80' * 3 + b'"', name='4-byte across piece')
case(['-c', '.'], b'[' + b'1,' * 40000 + b'2]', name='long array across pieces')
case(['-c', 'length'], b'{' + b','.join(b'"k%d":%d' % (i, i) for i in range(30000)) + b'}', name='object across pieces')

# Nesting: 10,000 levels read, 10,001 an error; objects count their names as jq's parser does.
def nest(o, c, n, mid=b'1'):
    return o * n + mid + c * n
for n in [9999, 10000, 10001]:
    case(['-c', 'length'], b'1 ' + nest(b'[', b']', n, b'') + b' 2', name=f'arrays {n}')
for n in [4999, 5000, 5001]:
    case(['-c', 'length'], nest(b'{"a":', b'}', n), name=f'objects {n}')
    case(['-c', 'length'], nest(b'{"a":', b'}', n, b'[]'), name=f'objects+[] {n}')
for n in [3333, 3334]:
    case(['-c', 'length'], nest(b'[{"a":', b'}]', n), name=f'mixed {n}')
case(['-c', 'length'], nest(b'[', b']', 9999, b'{"a":1}'), name='9999+{a:1}')
case(['-c', 'length'], nest(b'[', b']', 9999, b'{"a":[]}'), name='9999+{a:[]}')
case(['-c', 'length'], nest(b'[', b']', 9998, b'{"a":[]}'), name='9998+{a:[]}')
case(['-c', 'tojson | length'], nest(b'[', b']', 10000, b''), name='deep tojson')
case(['-c', '. == .'], nest(b'[', b']', 10000, b''), name='deep compare')
case(['-n', '-c', 'reduce range(20000) as $i (0; [.]) | tojson | length'], name='deep computed value')
case(['-n', '-c', '"[" * 10001 | fromjson?'], name='fromjson depth')
case(['-n', '-c', '"[" * 10000 + "]" * 10000 | fromjson | length'], name='fromjson 10000')

# Comparing values nested deeper than jq compares: "Comparison too deep", "Equality check too deep".
for n in [9999, 10000, 10001]:
    for w in ['[.]', '{a:.}', '[1,.]', '{a:{b:.}}']:
        for e in ['. == $a', '. < $a', '. != $a', '[., $a] | sort | length', '. == .', '[.,.] | sort | length',
                  '[., $a] | unique | length', '[[.], [$a]] | group_by(.[0]) | length', '[.] - [$a] | length',
                  '[., $a] | min | type', 'tojson | length']:
            case(['-n', '-c', f'(reduce range({n}) as $i (0;{w})) as $a | reduce range({n}) as $i (0;{w}) | {e}'],
                 name=f'deep compare {n} {w} {e}')
# A NUL byte right after a number is refused (jq reads the number as a C string).
for data in [b'1\x00', b'[1\x00]', b'12\x00 3', b'"a\x00"', b'1 \x00']:
    case(['-c', '.'], data)

# Recursion: a million deep, not in tail position; ten million in tail position.
case(['-n', 'def f: if . == 0 then 0 else (. - 1 | f) + 1 end; 1000000 | f'], name='def a million deep', timeout=120)
case(['-n', 'def f: if . < 10000000 then .+1|f else . end; 0|f'], name='tail ten million', timeout=120)
case(['-n', '[limit(3; range(0; infinite))]'], name='limit infinite')
case(['-n', '-c', 'reduce range(0;1000000) as $i ([]; . + [$i]) | length'], name='reduce append', timeout=120)
case(['-n', '-c', 'reduce range(0;300000) as $i (""; . + "a") | length'], name='reduce string append', timeout=120)
case(['-n', '-c', 'reduce range(0;300000) as $i ({}; .["k\\($i)"] = $i) | length'], name='reduce object', timeout=120)
case(['-n', '-c', '[range(0;1000000)] | sort | length'], name='sort a million', timeout=120)

# Assignments and paths.
for f in ['.[] |= empty', '.[1:] |= empty', '.a |= (.,10)', '.c += 1', '.a //= 5', '.d //= 5', '.a = (1,2)',
          '(.a,.b) = 9', '.[] += (1,2)', '.a |= empty', 'to_entries', 'del(.a, .b)', 'del(.[])', 'path(.a // .b)',
          '[paths]', 'path(1)?', 'path(null)?', '[path(.a | select(. == 1))]', 'path(if .a then .b else .c end)',
          'path(first(.a,.b))', 'path(limit(2; .a,.b,.c))', '[path(getpath(["x","y"]))]', 'path(last(.a,.b))?',
          '[path(.a[1:2])]?', 'path(.[]?)', '[path(..)]', 'path(.a | tostring)?', 'path(-.a)?',
          'path(.a, $__loc__)?', '.a |= . + 1', '.[] |= . * 2', 'map_values(. + 1)', '.e = .a', '.e |= .a?',
          'getpath(["a","b"])?', 'try getpath(["a","b"])', '.a[0] = 1?', 'setpath([]; 1)', 'delpaths([["a"],["b"]])',
          'delpaths([[]])', 'del(.)', 'path(.)', '[path(.a, .b)]', 'del(.a[0])?', '.a.b.c = 1?', 'path(.[1:3])?']:
    case(['-c', f], '{"a":1,"b":2}')
    case(['-c', f], '[1,2,3,[4,5]]')
    case(['-c', f], 'null')

# Regular expressions read alike by PCRE2 and Oniguruma.
for s, r, fl in [('abc', 'b', None), ('abc', '^b', None), ('abc', 'B', 'i'), ('a\nb', '^b', None), ('a\n', 'a$', None),
                 ('a\nb', 'a.b', None), ('a\nb', 'a.b', 'p'), ('a\nb', 'a.b', 's'), ('abc', 'a b c', 'x'),
                 ('café', 'caf.$', None), ('日本', '^..$', None), ('abc', '[', None), ('abc', 'b', 'q'),
                 ('abc', '', 'n'), ('aaa', 'a{2}', 'g'), ('x', 'x', 'l'), ('ab', '(?<n>a)b', None),
                 ('a' * 40 + 'b', '(a+)+$', None), ('a' * 30000 + 'b', '(a+)+$', None)]:
    fl_part = '' if fl is None else '; ' + repr(fl).replace("'", '"')
    case(['-c', f'test({repr(r)[1:-1]!r}{fl_part})'.replace("'", '"')], '"' + s.replace('\n', '\\n') + '"')
case(['-c', 'test("a")'], '1')
case(['-c', 'test(1)?'], '"a"')
case(['-c', '[.[] | test("a")]'], '["a","b","ca"]')

# Arguments.
case(['-n', '-c', '--arg', 'x', 'é', '$x'])
case(['-n', '-c', '--argjson', 'x', '{"a":1.50}', '$x, $x.a'])
case(['-n', '-c', '--argjson', 'x', '[1,', '$x'])
case(['-n', '-c', '--arg', 'x', '1', '--arg', 'y', '2', '[$x, $y]'])
case(['-n', '-c', '--arg', 'x'])
case(['-n', '-c', '$y'])
case(['-n', '-c', '--arg', 'x', '1', '[$x] | . as [$a] | $a'])
case(['-n', '-c', '--arg', 'x', '1', 'def f: $x; f'])
case(['.', '-c'], '[1,  2]')
case(['-c', '.', '--', '-x'], '1')
case(['-x', '.'], '1')
case([], '1')
case(['-c', '{'], '1')
case(['-c', 'foo'], '1')
case(['-c', '.a.[0]'], '{"a":[1]}')
case(['-c', '. as [$a] | $a'], '[1]')
case(['-c', 'reduce .[] as [$a] (0; .)'], '[[1]]')
case(['-c', '@sh'], '1')
case(['-c', 'try error("x") catch .'], '1')
case(['-c', 'label $f | 1'], '1')
case(['-c', '# a comment\n. # another\n'], '1')
case(['-c', '.a as $x | .b as $y | $x + $y'], '{"a":1,"b":2}')
case(['-c', '[.[] as $x | $x * 2]'], '[1,2]')
case(['-c', 'def f(x): x * 2; f(.)'], '3')
case(['-c', 'def f: def g: 3; g * 2; f'], 'null')
case(['-c', 'def f(g): def h: g; h; f(5)'], 'null')
case(['-c', 'def f($a): $a + a; f(1)'], 'null')
case(['-c', 'def f(a; b): [a, b]; f(1, 2; 3)'], 'null')
case(['-c', 'def f: reduce .[] as $x (0; . + $x); f'], '[1,2,3]')
case(['-c', 'def recurse_down: recurse; [recurse_down]'], '[[1]]')
case(['-c', 'def map(f): "mine"; map(.)'], '[1]')
case(['-c', '[.[] | {a: .x}]'], '[{"x":1},{"x":2}]')
case(['-c', '{(.[] | tostring): 1}'], '[1,2]')
case(['-c', '[.[] | "\\(.)-\\(. * 2)"]'], '[1,2]')
case(['-c', '{a: 1} | .a as $a | {$a, b: $a}'], 'null')
case(['-c', '{"a":1} | {a: .a | . + 1, b: -.a}'], 'null')
case(['-c', '{a: 1 + 2}'], 'null')
case(['-c', '{if: 1, then: 2, reduce: 3, and: 4}'], 'null')
case(['-c', '.and, .if, .end'], '{"and":1,"if":2,"end":3}')
case(['-c', '[.[] | if . > 1 then "big" elif . > 0 then "small" else "zero" end]'], '[0,1,2]')
case(['-c', 'reduce .[] as $x (0; . + $x) / length'], '[1,2,3]')
case(['-c', '[foreach .[] as $x (0; . + $x)]'], '[1,2,3]')
case(['-c', '[foreach .[] as $x (0; . + $x; [$x, .])]'], '[1,2,3]')
case(['-c', '[.[] | select(. > 1) | . * 10]'], '[1,2,3]')
case(['-c', '[.[] | numbers, strings]'], '[1,"a",null]')
case(['-c', '[limit(3; .[])]'], '[1,2,3,4,5]')
case(['-c', '[first(.[]), first(empty)]'], '[1,2]')
case(['-c', '[.[] | tostring]'], '[1,"1",[1],{"a":1.0}]')
case(['-c', 'map(tojson)'], '[1,"1",[1],{"a":1.0}]')
case(['-c', '[.[] | tonumber?]'], '["1","1.5","x","1e2","-0","  2"]')
case(['-c', 'map(tonumber?)'], '["1.","01","+1",".5","nan","infinity","-inf","1e1000"]')
case(['-c', '.. |= (if type == "number" then . + 1 else . end)'], '[1,[2,{"a":3}]]')
case(['-c', '[.. | select(type == "number")]'], '[1,[2,{"a":3}]]')
case(['-c', '[recurse(.[]?; type != "number")]'], '[1,[2]]')
case(['-c', '[recurse(if . < 3 then . + 1 else empty end)]'], '0')
case(['-c', 'with_entries(.key |= ascii_upcase)'], '{"a":1,"b":2}')
case(['-c', 'to_entries'], '{"a":1,"b":[2]}')
case(['-c', 'from_entries'], '[{"key":"a","value":1},{"name":"b","value":2},{"Key":"c","Value":3},{"key":false,"name":"d"}]')
case(['-c', 'from_entries'], '[{"k":"a","v":1}]')
case(['-c', 'from_entries'], '[{"key":1,"value":1}]')
case(['-c', 'from_entries'], '[{"key":"a"},{"key":"a","value":2}]')
case(['-c', 'from_entries'], '[null]')
case(['-c', 'from_entries'], '[1]')
case(['-c', 'add'], '[[1],[2,3]]')
case(['-c', 'add'], '["a","b"]')
case(['-c', 'add'], '[{"a":1},{"b":2},{"a":3}]')
case(['-c', 'add'], '[1,null,2]')
case(['-c', 'add(.[] | . * 2)'], '[1,2]')
case(['-c', 'add(empty)'], '[1,2]')
case(['-c', 'any, all'], '[true,false]')
case(['-c', 'any(. > 1), all(. > 1)'], '[1,2]')
case(['-c', 'any(.[]; error("x"))?'], '[1,2]')
case(['-c', 'isempty(empty)?'], '1')
case(['-c', 'group_by(.a) | map(length)'], '[{"a":1},{"a":2},{"a":1}]')
case(['-c', 'unique_by(length)'], '["a","bb","c","dd","eee"]')
case(['-c', 'min_by(.a), max_by(.a)'], '[{"a":1,"b":1},{"a":1,"b":2},{"a":0,"b":3},{"a":0,"b":4}]')
case(['-c', 'min, max'], '[[1],[0],[1],[0]]')
case(['-c', 'sort_by(.a, .b)'], '[{"a":1,"b":2},{"a":1,"b":1},{"a":0,"b":5}]')
case(['-c', 'sort_by(.a; .b)?'], '[]')
case(['-c', 'reverse'], '"abc"')
case(['-c', 'reverse'], '""')
case(['-c', 'reverse'], 'null')
case(['-c', 'reverse'], '{}')
case(['-c', 'reverse'], '{"a":1}')
case(['-c', 'reverse'], '0')
case(['-c', 'reverse'], '2')
case(['-c', 'reverse'], 'true')
case(['-c', 'reverse'], '[1,[2],3]')
case(['-c', 'length'], '-5')
case(['-c', 'length'], '-0')
case(['-n', '-c', '[0] | .[0] * -1 | length'])
case(['-c', 'utf8bytelength'], '"aé日😀"')
case(['-c', 'ascii_downcase, ascii_upcase'], '"aBcÉ"')
case(['-c', 'ltrimstr("a"), rtrimstr("c"), ltrimstr(1), startswith("a"), endswith("c")'], '"abc"')
case(['-c', 'ltrimstr("a")'], '1')
case(['-c', 'startswith("a")'], '1')
case(['-c', 'split(", ")'], '"a, b, c"')
case(['-c', 'split(1)?'], '"a"')
case(['-c', 'join(", ")'], '["a","b"]')
case(['-c', 'join(null), join(1)?'], '[1,2]')
case(['-c', 'join(",")'], '[[1]]')
case(['-c', 'join(",")'], '{"a":"x","b":"y"}')
case(['-c', '@csv'], '[1,"a\\"b",null,true,1.5,1.000]')
case(['-c', '@tsv'], '[1,"a\\tb\\\\c\\nd\\re",null,true,1.5]')
case(['-c', '@csv'], '[[1]]')
case(['-c', '@csv'], '{"a":1}')
case(['-c', '@base64, @base64d?'], '"é✓"')
case(['-c', '@uri'], '"é/ ~-_.!*\'()"')
case(['-c', '@text, @json'], '{"a":[1,"x"]}')
case(['-c', '@base64d'], '"YWJ"')
case(['-c', '@base64d'], '"Y"')
case(['-c', '@base64d'], '"!!!!"')
case(['-c', '@base64d'], '"YW=J"')
case(['-c', '@base64d'], '"gA=="')
case(['-c', '@base64d'], '1')
case(['-c', 'tojson'], '"\\u0000é\\u007f"')
case(['-c', 'fromjson'], '"[1, 2]"')
case(['-c', 'fromjson'], '"nan"')
case(['-c', 'fromjson'], '" 1 "')
case(['-c', 'fromjson'], '""')
case(['-c', 'fromjson'], '"1 2"')
case(['-c', 'fromjson'], '"\\"\\\\ud800\\""')
case(['-c', 'tostring, tojson'], '"x"')
case(['-c', 'type'], '[null,true,1,"a",[],{}]')
case(['-c', 'map(type)'], '[null,true,1,"a",[],{}]')
case(['-c', 'map(length)?'], '[null,"ab",[1],{"a":1},-3]')
case(['-c', 'keys'], '[5,6]')
case(['-c', 'keys'], '1')
case(['-c', 'has("a")'], '[1]')
case(['-c', 'map(has(0))'], '[[1],[]]')
case(['-c', '.[] as $k | {"a":1} | has($k)'], '["a","b"]')
case(['-c', 'map(in({"a":1}))'], '["a","b"]')
case(['-c', '.[2:4] = ["x","y","z"]'], '[0,1,2,3,4,5]')
case(['-c', '.[2:4] |= map(. * 10)'], '[0,1,2,3,4,5]')
case(['-c', 'del(.[2:4])'], '[0,1,2,3,4,5]')
case(['-c', 'del(.[0,2,4])'], '[0,1,2,3,4,5]')
case(['-c', 'del(.[-1])'], '[0,1,2,3,4,5]')
case(['-c', 'to_entries | map(select(.value > 2)) | from_entries'], '{"a":1,"b":3,"c":5}')
case(['-c', '[splits(", ")]?'], '"a, b"')
case(['-c', '[.[] | floor, sqrt]'], '[4.5,-1.5,16]')
case(['-c', 'floor'], '"x"')
case(['-c', '[range(.)]'], '3')
case(['-c', '[range(.[0]; .[1])]'], '[1,4]')
case(['-c', '[range(0; 1; 0.25)]'], 'null')
case(['-c', '[range(0; 10; 0)]'], 'null')
case(['-c', '[range(1.5)]'], 'null')
case(['-c', '[range("a")]'], 'null')
case(['-c', '[range(1,2; 3,4)]'], 'null')
case(['-c', '[range(0; 6; 2, 3)]'], 'null')
case(['-c', '[limit(1,2; 1,2,3)]'], 'null')
case(['-c', '[limit(-1; 1,2)]'], 'null')
case(['-c', '[limit("a"; 1,2)]'], 'null')
case(['-c', '[limit(null; 1,2)]'], 'null')
case(['-c', '[limit(1.5; 1,2,3)]'], 'null')
case(['-c', '[.[] | [setpath(["a"]; 1,2)]]'], '[{}]')
case(['-c', '[setpath(["a"],["b"]; 1,2)]'], '{}')
case(['-c', '[test("A","B"; "i","x")]'], '"abc"')
case(['-c', '[has("a","b")]'], '{"a":1}')
case(['-c', '[.[(0,1)]]'], '[5,6]')
case(['-c', '[.[(0,1)][(0,1)]?]'], '[[1,2],[3]]')
case(['-c', '[.[(0,1):(2,3)]]'], '[1,2,3,4]')
case(['-c', '[(1,2) + (10,20)]'], 'null')
case(['-c', '[{(\"a\",\"b\"): (1,2)}]'], 'null')
case(['-c', '["\\(1,2) \\(3,4)"]'], 'null')
case(['-c', '[(1,2) as $x | ($x, 3)]'], 'null')
case(['-c', '[(1,2) and (true,false)]'], 'null')
case(['-c', '[{a:(1,2)} | .a]'], 'null')
case(['-c', '[(1, error("x"), 2)?]'], 'null')
case(['-c', '[(1,2)? | error("x")]'], 'null')
case(['-c', '(1 // 2) | error("y")'], 'null')
case(['-c', '[(null, error("x"), 3) // 4]'], 'null')
case(['-c', '[(null,false) // 4]'], 'null')
case(['-c', '[error("x") // 4]'], 'null')
case(['-c', '[empty // 4]'], 'null')
case(['-c', '[.[] // 4]'], '[null, 1, false, 2]')
case(['-c', 'reduce range(5) as $x (0; empty)'], 'null')
case(['-c', 'reduce range(3) as $x (0; .+1, .+10)'], 'null')
case(['-c', '[reduce (1,2) as $x (0,10; .+$x)]'], 'null')
case(['-c', 'reduce (1,2) as $x (0; .+$x) + 1'], 'null')
case(['-c', '[foreach (1,2) as $x (0,10; .+$x)]'], 'null')
case(['-c', '[foreach (1,2) as $x (0; .+$x, .*10)]'], 'null')
case(['-c', 'if true then {a:1} else 2 end.a'], 'null')
case(['-c', '[.[] | -.]'], '[1,-1.000,0,-0,1e1000]')
case(['-c', '-"a"'], 'null')
case(['-c', '[.[] | . == 1]'], '[1, 1.0, 1.00, 1e0, 10e-1, 0.1e1]')
case(['-c', 'map(. < 2)'], '[1.9999999999999999999, 2.0000000000000000001]')
case(['-c', '.[0] < .[1]'], '[1.9999999999999999999, 1.99999999999999999991]')
case(['-c', '.[0] < .[1]'], '[1.9999999999999999999, 2]')
case(['-c', 'sort'], '[3, 1.0, 1, -0, 0, 1e2, 99.99]')
case(['-c', '.[0] + .[1]'], '[100000000000000000001, 1]')
case(['-c', '.[]'], '[1e1000]')
case(['-c', '.[] | . * 1'], '[1e1000]')
case(['-c', 'tostring'], '1e1000')


# More of the language: closures, scopes, recursion, generators inside everything.
MORE = [
    ('def f(g): def h: g; [h, (1 as $x | h)]; 5 as $x | f($x)', 'null'),
    ('def f(g): g | g; f(. * 2)', '3'),
    ('def f($n): if $n == 0 then [] else [$n] + f($n - 1) end; f(5)', 'null'),
    ('def fib: if . < 2 then . else (. - 1 | fib) + (. - 2 | fib) end; [range(15) | fib]', 'null'),
    ('def ack(m; n): if m == 0 then n + 1 elif n == 0 then ack(m - 1; 1) else ack(m - 1; ack(m; n - 1)) end; ack(2; 3)', 'null'),
    ('def f: def f: 2; f * 10; f', 'null'),
    ('def f: 1; def g: f + 1; def f: 10; [f, g]', 'null'),
    ('. as $x | def f: $x * 2; 3 as $x | f', '5'),
    ('def f(x): x as $v | [$v, x]; f(1, 2)', 'null'),
    ('def r: if . > 0 then ., (. - 1 | r) else . end; [5 | r]', 'null'),
    ('def map2(f): [.[] | f]; map2(. + 1)', '[1,2]'),
    ('[.[] | def sq: . * .; sq]', '[1,2,3]'),
    ('reduce .[] as $x ({}; .[$x.k] += [$x.v])', '[{"k":"a","v":1},{"k":"b","v":2},{"k":"a","v":3}]'),
    ('[foreach .[] as $x ({}; .[$x] += 1; .)]', '["a","b","a"]'),
    ('[foreach range(5) as $i (null; $i; select(. % 2 == 0))]', 'null'),
    ('[.[] | select(.a?) | .a]', '[{"a":1},2,{"a":null},{"a":false},{"a":"x"}]'),
    ('[..|numbers]', '{"a":[1,{"b":2}],"c":3}'),
    ('[paths]', '{"a":[1,{"b":2}],"c":3}'),
    ('[paths | tostring]', '[[[]]]'),
    ('[path(..)]', '[[1]]'),
    ('del(.. | select(. == 2))', '{"a":[1,2,{"b":2}],"c":2}'),
    ('del(.[] | select(. > 1))', '[1,2,3,1]'),
    ('to_entries', '[[1],{"a":1}]'),
    ('with_entries(.value |= . * 2)', '{"a":1,"b":2}'),
    ('.a += 1 | .b -= 1 | .c *= 2 | .d /= 2 | .e %= 2', '{"a":1,"b":1,"c":1,"d":1,"e":3}'),
    ('.[] += 10', '{"a":1,"b":2}'),
    ('.a[1:3] = ["x"]', '{"a":[1,2,3,4]}'),
    ('.a[1:3] |= reverse', '{"a":[1,2,3,4]}'),
    ('.a |= sort', '{"a":[3,1,2]}'),
    ('.b = .a', '{"a":[1]}'),
    ('.["new"] = 1 | keys_unsorted', '{"z":0,"a":1}'),
    ('.z = 5 | keys_unsorted', '{"z":0,"a":1}'),
    ('del(.z) | .z = 5 | keys_unsorted', '{"z":0,"a":1}'),
    ('. + {"z": 9} | keys_unsorted', '{"z":0,"a":1}'),
    ('{"q": 1} + . | keys_unsorted', '{"z":0,"a":1}'),
    ('. * {"a": {"x": 1}}', '{"z":0,"a":{"y":2}}'),
    ('-S', '{"b":{"z":1,"é":2,"a":3},"A":[{"y":1,"x":2}],"_":0}'),
    ('tostring', '"\u0001\t\u007f"'),
    ('@text', '"\u0001\t\u007f"'),
    ('[.[] | @json]', '["\u0001",1.50,null]'),
    ('ascii_downcase | ascii_upcase', '"Ünïcödé ABC xyz"'),
    ('[splits(",")]?', '"a,b"'),
    ('[.[] | length]', '["", "é", "日本", "😀", "a\u0000b"]'),
    ('[.[] | utf8bytelength]', '["", "é", "日本", "😀"]'),
    ('[.[1:2], .[:1], .[-1:]]', '"日本語"'),
    ('.[2:]', '"😀ab"'),
    ('test("^\\\\d+$")', '"12345"'),
    ('[.[] | test("[a-z]+"; "x")]', '["abc","123"]'),
    ('[.[] | test("ü"; "i")]', '["Ü","u"]'),
    ('[.[] | startswith("é")]', '["éa","ae"]'),
    ('split("é")', '"aébéc"'),
    ('join("é")', '["a","b"]'),
    ('[.[] | tonumber]', '["1.000","-0","1e2","0.1e1","100","2.50"]'),
    ('[.[] | tostring]', '[1.000,-0,1e2,0.1e1,100,2.50]'),
    ('map(. + 1)', '[1.000,-0,1e2,0.1e1,100,2.50]'),
    ('map(-.)', '[1.000,-0,1e2,0.1e1,100,2.50,0.0]'),
    ('sort', '[1.000,-0,1e2,0.1e1,100,2.50,0.0,1]'),
    ('unique', '[1.000,-0,1e2,0.1e1,100,2.50,0.0,1]'),
    ('group_by(.)', '[1.000,-0,1e2,0.1e1,100,2.50,0.0,1]'),
    ('min, max', '[1.000,1,1.0]'),
    ('[.[] == 1]', '[1.000,1,1.0,0.1e1,10e-1,1.0000000000000000001]'),
    ('. == {"a":1}', '{"a":1.0}'),
    ('[limit(5; repeat(.))]?', '1'),
    ('[range(3)] | [.[] | tostring] | join(",")', 'null'),
    ('[range(0; 1; 0.3)]', 'null'),
    ('[range(1; 0; -0.3)]', 'null'),
    ('[range(-3)]', 'null'),
    ('[range(2.5; 5)]', 'null'),
    ('[limit(3; .[])]', '[1,2]'),
    ('[first(range(10; 0; -1))]', 'null'),
    ('[.[] as $x | $x | select($x > 1)]', '[1,2,3]'),
    ('. as $d | [paths] | map(. as $p | $d | getpath($p))', '{"a":[1,{"b":2}]}'),
    ('[getpath(["a",0], ["a",1,"b"], ["x"])]', '{"a":[1,{"b":2}]}'),
    ('[paths] as $ps | reduce $ps[] as $p (.; setpath($p; 0))', '{"a":[1,{"b":2}]}'),
    ('tojson | fromjson == .', '{"a":[1.50,{"b":"é\u0000"}],"c":1e1000}'),
    ('[.[] | tojson | fromjson]', '[1.000,"x",null,[1e-7]]'),
    ('.. |= (if type == "string" then ascii_upcase else . end)', '{"a":["x",{"b":"y"}]}'),
    ('[.. | strings]', '{"a":["x",{"b":"y"}]}'),
    ('[.[] | (.a // "none")]', '[{"a":1},{"a":null},{},{"a":false}]'),
    ('[.[] | .a // .b // "none"]', '[{"a":1},{"b":2},{}]'),
    ('.a // error("missing")', '{}'),
    ('[.[] | (.a? // "x")]', '[1,{"a":2}]'),
    ('[.[] | try .a]', '[1,{"a":2}]'),
    ('[.[] | .a?]', '[1,{"a":2},null,"s"]'),
    ('[.[] | .[0]?]', '[1,[2],null,"s",{"a":1}]'),
    ('[.[] | .[]?]', '[1,[2],null,"s",{"a":1}]'),
    ('[.[] | .[1:]?]', '[1,[2,3],null,"st",{"a":1}]'),
    ('[.[]?.a]', '[{"a":1},2]'),
    ('[.[].a?]', '[{"a":1},2]'),
    ('[(.[] | .a)?]', '[{"a":1},2,{"a":3}]'),
    ('.a.b?', '{"a":5}'),
    ('(.a.b)?', '{"a":5}'),
    ('try (.a.b)', '{"a":5}'),
    ('.a?.b', '{"a":5}'),
    ('[.[] | numbers | . * 2]', '[1,"a",2]'),
    ('{"a":1,"b":2} | {a, b, c: .a}', 'null'),
    ('{(.[] | tostring): .[]}', '[1,2]'),
    ('[{a: .[]} | .a]', '[1,2]'),
    ('{a: 1} | {a: .a, a: 2}', 'null'),
    ('{"a b": 1} | {"a b"}', 'null'),
    ('{"\\(1+1)": 3}', 'null'),
    ('{$__loc__}', 'null'),
    ('[.[] | {x: ., y: (. * 2)}]', '[1,2]'),
    ('"\\(.a) \\(.b | length) \\([.c])"', '{"a":"x","b":[1,2],"c":null}'),
    ('@csv "\\(.)" , @tsv "\\(.)", @json "\\(.)", @text "\\(.)", @base64 "\\(.)", @uri "\\(.)"', '[1,"a b"]'),
    ('[.[] | @base64]', '["", "a", "ab", "abc", "abcd", "é"]'),
    ('[.[] | @base64d]', '["", "YQ==", "YWI=", "YWJj", "YWJjZA==", "w6k=", "YQ", "YWI"]'),
    ('[.[] | @uri]', '["a b", "ü", "~._-", "a+b=c&d"]'),
    ('[.[] | @csv]', '[[1,"a"],["x\\"y"],[null,true,false]]'),
    ('[.[] | @tsv]', '[[1,"a"],["x\\ty"],[null,true,false]]'),
    ('add', '[[1],[2,3],[]]'),
    ('add / length', '[1,2,3,4]'),
    ('[.[] | add]', '[[1,2],["a","b"],[[1],[2]],[{"a":1},{"b":2}],[],[null]]'),
    ('map(add)', '[[1,null],[null,"a"]]'),
    ('any(.[]; . > 2)', '[1,2,3]'),
    ('all(.[]; . > 0)', '[1,2,3]'),
    ('[any(.[]; .), all(.[]; .)]', '[]'),
    ('[.[] | not]', '[true,false,null,0,"",[],{}]'),
    ('[.[] | if . then 1 else 0 end]', '[true,false,null,0,"",[],{}]'),
    ('[.[] | . and true, . or false]', '[true,false,null,0]'),
    ('[.[] | type]', '[true,false,null,0,"",[],{},1.5]'),
    ('[.[] | length]', '[null,"ab",[1,2,3],{"a":1},-1.5]'),
    ('keys', '{"b":1,"a":2,"é":3,"Z":4}'),
    ('[.[] | has("a")]', '[{"a":null},{"b":1}]'),
    ('[.[] | in({"a":1})]', '["a","b"]'),
    ('reverse', '[1,2,3]'),
    ('[.[] | floor]', '[1.5,-1.5,0,-0.5,1e300]'),
    ('[.[] | sqrt]', '[4,2,0,0.25]'),
    ('[.[] | -.]', '[1.5,-1.5,0,-0.5,1e300]'),
    ('[.[] | . * 3, . / 3, . % 3]', '[7,-7,7.5]'),
    ('[.[] / 0]?', '[1]'),
    ('[1, 2] | .[0] / .[1]', 'null'),
    ('[.[] | tostring | length]', '[0.1,0.123456789012345678,1e100,123456789,1.5e-7]'),
    ('map(. * 1)', '[0.1,0.123456789012345678,1e100,123456789,1.5e-7,1e-5,1e-4,9007199254740993,1e15,1e16,123456789012345678]'),
    ('map(. / 3)', '[1,2,10,1e20,1e-20,7]'),
    ('map(. * 1e10)', '[1,2,3.3,4.44]'),
    ('[range(10)] | map(. / 10)', 'null'),
    ('[range(20)] | map(pow(10; .))?', 'null'),
    ('[1e15, 1e16, 1e17, 1.5e16, 123e14] | map(. + 0)', 'null'),
    ('[0.0001, 0.00001, 0.000123, 0.0000123] | map(. + 0)', 'null'),
    ('[1e308 * 10, -1e308 * 10] | map(. + 0)', 'null'),
    ('[5e-324, 2.2250738585072014e-308, 4.9e-324] | map(. + 0)', 'null'),
]
for f, d in MORE:
    if f.startswith('-'):
        case([f, '-c', '.'], d, name='more ' + f)
    else:
        case(['-c', f], d, name='more ' + f)

# Doubles, written as jq writes them: the shortest digits that read back the same.
import random
random.seed(1808)
vals = []
for _ in range(3000):
    e = random.randint(-330, 310)
    m = random.random() * 10
    v = m * 10.0 ** e if -300 < e < 300 else m * (10.0 ** (e // 2)) * (10.0 ** (e - e // 2))
    if v == v and abs(v) != float('inf'):
        vals.append(repr(v) if random.random() < 0.5 else repr(-v))
for k in range(0, len(vals), 500):
    chunk = '[' + ','.join(vals[k:k + 500]) + ']'
    case(['-c', 'map(. + 0)'], chunk, name=f'doubles {k}')
    case(['-c', 'map(. * 3)'], chunk, name=f'doubles*3 {k}')
ints = [str(random.randint(-10**18, 10**18)) for _ in range(500)] + [str(2**k) for k in range(0, 70)] + [str(10**k) for k in range(0, 25)]
case(['-c', 'map(. + 0)'], '[' + ','.join(ints) + ']', name='integers as doubles')
case(['-c', 'map(. + 0.5)'], '[' + ','.join(ints) + ']', name='integers plus a half')

# Literals of more than 17 digits: jq rounds them to 17 digits (half to even) before reading a
# double, so comparing them with doubles and computing with them follows that.
random.seed(7)
for trial in range(3):
    nums = []
    for _ in range(1500):
        digits = random.randint(15, 25)
        coef = str(random.randint(10**(digits-1), 10**digits - 1))
        e = random.randint(-40, 40)
        nums.append(f'{coef}e{e}' if random.random() < 0.5 else f'-{coef[:5]}.{coef[5:]}e{e}')
    data = '[' + ','.join(nums) + ']'
    for f in ['map(. + 0)', 'map(. * 7)', 'sort', 'map(. < 1e10)', '[.[] | . == (. + 0)]']:
        case(['-c', f], data, name=f'long literals {trial} {f}')

# Ties: a double halfway between the two shortest neighbours that read back as it is written with
# the even one, as jq's dtoa writes it (Rust's own shortest digits take the upper one).
random.seed(11)
ties = []
for _ in range(4000):
    kind = random.random()
    if kind < 0.5:
        v = random.randint(10**14, 2 * 10**16) + random.randint(0, 15) / 16
    else:
        v = random.randint(1, 10**6) * 2.0 ** random.randint(-60, 60)
    ties.append(repr(v if random.random() < .5 else -v))
case(['-c', 'map(. * 1)'], '[' + ','.join(ties) + ']', name='ties')
case(['-n', '-c', '[1e15 + 0.3, 0.1 + 0.2, 2.5e-308 * 3]'])

# jq 1.8.2's grammar: `Expr as $x | Query` binds a whole expression and may stand after `,`, taking
# the rest; `-` before a term makes a term; `reduce` and `foreach` read from an expression, `try`
# takes a term.
GRAMMAR = ['-1 as $x | $x, 2', '1 + -1 as $x | $x', 'reduce -1 as $x (0; . + $x)', '-1 as $x | -$x as $y | $y', '1 - -1 as $x | $x', '1 + .a as $x | $x', '1 + .a as $x | $x | . * 2', '[1, .a as $x | $x, 5]', '.a // .b as $x | $x', '1 as $x | 2 + -$x as $y | $y', '.a == .a as $x | $x', 'not or .a as $x | $x', '{x: .a as $v | $v}', '.a |= . as $x | $x + 1', 'if .a as $x | $x then 1 else 2 end', '[.[] | -. as $n | $n]', '1, .a as $x | $x, 9 | . + 1', '- 1 + 2', '-1 * 3', '- .a * 2', '-.a | . + 1', '[-1, 2]', '-(1,2)', '- -1', '-.a? // 7', '-[1][0]', '-"a"?', '.a as $x | -$x', '- reduce (1,2) as $x (0; .+$x)', '-1 | 2', '- 1 as $x | $x, 2', '-.a as $x | $x, 2', '-(1) as $x | $x, 2', 'reduce -.a as $x (0; . + $x)', 'reduce .a + 1 as $x (0; . + $x)', 'reduce .b[] * 2 as $x (0; . + $x)', 'foreach .b[] + 1 as $x (0; . + $x)', '-1e2 as $x | $x, 2', '[limit(1; -1 as $x | $x, 2)]', '1, def f: 2; f, 3', '[.b[] as $x | $x * 2]', 'try -1', 'try .a + 1', 'try error("x") // 5', '[.b[] | try -. ]', '.a as $x | .b | map(. + $x)', '.b | .[0] as $x | .[1] as $y | $x + $y', '"\\(.a as $x | $x + 1)"', '[.b[] | . as $x | select($x > 1)]', 'def f: .a as $x | $x; f, 1', '(.a as $x | $x) + 1', '.a as $x | $x as $y | $y', '1 as $x | 2 as $y | $x + $y, 3', '.a + 1 as $x | $x | . + 1, 0', '.b[0] = 5 as $x | $x', '.a // empty as $x | $x', '[.b[] as $x | $x] | length', '[.[] as $x | $x]', '.b | first(.[] as $x | $x)']
for f in GRAMMAR:
    case(['-c', f], '{"a":3,"b":[1,2]}')

# Object keys that are constants, folded as jq folds them when it reads the filter: one that is not
# a text is refused then (exit 3); `-`, a pipe or `,` is not folded, and fails when run (exit 5).
KEYS = ['{(1+1): 2}', '{(-1): 2}', '{([1,2]): 1}', '{([1,.]): 1}', '{({"a":1}): 1}', '{({a:1}): 1}', '{(1 == 1): 1}', '{("a" + "b"): 1}', '{("a" * 2): 1}', '{(1, "a"): 1}', '{(null): 1}', '{(true): 1}', '{((1)): 1}', '{(1 | 2): 1}', '{("\\(1)"): 1}', '{(1 / 0): 1}', '{([]): 1}', '{({}): 1}', '{([[1]]): 1}', '{(1+1+1): 2}', '{(.): 2}', '{(1 as $x | 2): 1}', '{(@base64): 1}', '{(1E1000): 1}', '{(1+"a"): 1}', '{([1]+[2]): 1}', '{({a:1}*{b:2}): 1}', '{("a" < "b"): 1}', '{({(1+1|tostring): 1}): 1}', '{({"a":1} | .a): 1}', '{([1,2] | length): 1}', '{(([1]) + ([2])): 1}', '{("x" / "y"): 1}', '{([1,(2,3)]): 1}', '{({a:1,b:[2]}): 1}', '{({a:.}): 1}', '{("a" + "b"): 1, (2+2): 3}', '{([(1,2),3]): 1}', '{([1,(2,3)]): 1}', '{([(1,2),(3,4)]): 1}', '{([((1,2),3),4]): 1}', '{({("a"+"b"): 1}): 2}', '{({("a"): 1}): 2}', '{({a: (1,2)}): 1}']
for f in KEYS:
    case(['-c', f], '5')

# A value made by a filter that is not a path stands in a path where its input does when it is that
# input (jq's `jv_identical`); `last(f)` of no output has none.
for doc in ['{"a":1}', '"s"', 'true', 'null', '{"a":null,"b":null}', '[1,2]', '{"a":{"x":1},"b":{"x":1}}']:
    for f in ['[path(reduce empty as $x (.; .))]', '[path(last(.))]', '[path(last(.a,.b))]', '[path(last(empty))]',
              '[last(empty)]', '[path(. as $v | $v)]', '[path(reduce (.a,.b) as $x (.; .c))]',
              '[path(last(.a,.b)|.c)]', '[path(last(.[]))]', '[path(foreach (1,2) as $x (.; .))]',
              '[path(if true then . else 1 end)]', '[path(null)]', '[path(.a | 1)]']:
        case(['-c', f], doc)

# A part of a byte order mark: at the start of the input and alone it is nothing, after anything
# else an error.
for doc in [b' \xef', b' \xef\xbb', b'1 \xef', b'\xef', b'\xef\xbb', b'\xef\xbb\xbf', b'\xef\xbb\xbf\xef',
            b'\xef1', b'\n\xef', b'\xef\xbb\xbf 2', b'\t\xef\xbb\xbf']:
    case(['-c', '.'], doc, name='part of a mark ' + repr(doc))

# Ending early: a large input cut short by `head`, `first(inputs)` and `limit` on it.
for cmd in ['bash -c \'{prog} -n "range(100000000)" | head -2; echo "status ${{PIPESTATUS[0]}}"\'',
            'bash -c \'{prog} -c ".[]" {here}/records-2.json | head -1; echo "status ${{PIPESTATUS[0]}}"\'',
            'bash -c \'{prog} -n "[limit(3; inputs)]" {here}/lines-2.ndjson | wc -c\'']:
    case([], b'', name='early: ' + cmd, shell=('raw', cmd))
# Output ending early: limit, first(inputs), head -1 on a large input, a closed pipe.
case(['-n', '-c', 'first(inputs)'], '1 2 3 [')
case(['-n', '-c', '[limit(2; inputs)]'], '1 2 3 [')
case(['-n', '-c', 'input, input'], '1 2 3 [')

# Files: one stream, a missing one told and the main loop stopping after the value being read.
with open(os.path.join(HERE, 'three.json'), 'w') as f:
    f.write('1 2 3')
with open(os.path.join(HERE, 'one.json'), 'w') as f:
    f.write('[1]\n')
for cmd in ['{prog} -c . nope {here}/three.json', '{prog} -c . {here}/three.json nope {here}/three.json',
            '{prog} -c "[., input]" nope {here}/three.json', '{prog} -c . {here}/three.json nope',
            '{prog} -c . nope', '{prog} -nc "[inputs]" nope {here}/three.json {here}/three.json',
            '{prog} -sc . nope {here}/three.json', '{prog} -e . nope {here}/one.json',
            '{prog} -c . {here}/three.json {here}/one.json {here}/three.json', '{prog} -c . {here}/one.json {here}/one.json',
            '{prog} -c . {here}', 'printf "[1,2]" | {prog} -c .[] - ', 'printf "1 2" | {prog} -c "first(inputs)"',
            '{prog} -n "[limit(3; inputs)]" {here}/three.json {here}/three.json',
            'printf "[1,2,3]" | {prog} .[] >> {here}/append.out; printf "[4]" | {prog} -c . >> {here}/append.out; cat {here}/append.out; rm {here}/append.out']:
    case([], b'', name='files: ' + cmd, shell=('raw', cmd))

# The large inputs.
RECORDS = os.path.join(HERE, 'records-2.json')
LINES = os.path.join(HERE, 'lines-2.ndjson')
NUMBERS_FILE = os.path.join(HERE, 'numbers-2.json')


def make_large_inputs():
    """Makes the three inputs of 2 MB with `json-input` when they are not there yet."""
    if all(os.path.exists(f) for f in (RECORDS, LINES, NUMBERS_FILE)):
        return True
    sg = os.path.join(REPO, 'interpreter', 'target', 'release', 'sg')
    if not os.path.exists(sg):
        print('the large inputs are left out: interpreter/target/release/sg is not built')
        return False
    maker = os.path.join(HERE, 'json-input')
    subprocess.run([sg, 'build', os.path.join(REPO, 'trials', 'inputs', 'json.sg'), '--release', '-o', maker],
                   check=True, timeout=600)
    for kind, path in (('records', RECORDS), ('lines', LINES), ('numbers', NUMBERS_FILE)):
        with open(path, 'wb') as out:
            subprocess.run([maker, kind, '2', '7'], stdout=out, stdin=subprocess.DEVNULL, check=True, timeout=600)
    return True


if make_large_inputs():
    for f in [['.'], ['-c', '.'], ['-c', '.[] | select(.active and .score > 5000) | {id, name, city: .address.city}'],
              ['group_by(.address.city) | map({city: .[0].address.city, n: length, avg: (map(.score) | add / length)})'],
              ['sort_by(.balance) | .[0], .[-1]'], ['-r', '.[] | [.id, .name, .score] | @csv'],
              ['-r', '.[] | [.id, .name, .note] | @tsv'], ['reduce .[] as $r ({}; .[$r.address.city] += 1)'],
              ['-S', '.[0:3]'], ['--tab', '.[0:2]'], ['-c', '[paths] | length'], ['-c', '[.. | strings] | length'],
              ['-c', 'map(.tags | join(","))'], ['-c', 'map(.note | test("café"))  | map(select(.)) | length'],
              ['-c', 'map(.note | @base64 | @base64d) == map(.note)'], ['-c', 'tojson | fromjson | length'],
              ['-c', 'map(del(.history)) | .[0:2]'], ['-c', '.[].address.geo |= map(. * 2) | .[0]'],
              ['-c', 'map(to_entries | from_entries) == .'], ['-c', '[.[] | .score] | add'], ['-c', 'unique_by(.name) | length']]:
        case(f, open(RECORDS, 'rb').read() if False else b'', name='records ' + ' '.join(f), shell=(f, RECORDS))
    case(['-c', 'select((.tags | length) > 2) | .tags'], b'', name='lines tags', shell=(['-c', 'select((.tags | length) > 2) | .tags'], LINES))
    case(['-c', '.'], b'', name='lines .', shell=(['-c', '.'], LINES))
    case(['-c', '[.. | numbers] | add'], b'', name='numbers add', shell=(['-c', '[.. | numbers] | add'], NUMBERS_FILE))
    case(['.'], b'', name='numbers .', shell=(['.'], NUMBERS_FILE))
    case(['-c', '.'], b'', name='records + missing + lines', shell=(['-c', '.'], RECORDS + ' /nonexistent ' + LINES))
    case(['-c', '.'], b'', name='head -1', shell=('head1', RECORDS))
    case(['-c', '.'], b'', name='cut off', shell=('cut', RECORDS))
    case(['-c', '.'], b'', name='bad mid-stream', shell=('mid', LINES))


def run(prog, argv, stdin, timeout, shell):
    if shell is None:
        p = subprocess.run([prog] + argv, input=stdin, capture_output=True, timeout=timeout)
        return p.stdout, p.returncode
    spec, path = shell
    if spec == 'raw':
        cmd = path.format(prog=prog, here=HERE)
    elif spec == 'head1':
        cmd = f'{prog} -c .[] {path} | head -1'
    elif spec == 'cut':
        cmd = f'head -c 1000000 {path} | {prog} -c length'
    elif spec == 'mid':
        cmd = f'(head -c 500000 {path}; echo "{{bad"; cat {path}) | {prog} -c .id'
    else:
        quoted = ' '.join("'" + a.replace("'", "'\\''") + "'" for a in spec)
        cmd = f'{prog} {quoted} {path}'
    p = subprocess.run(['/bin/sh', '-c', cmd + '; echo "exit=$?" >&2'], capture_output=True, timeout=timeout)
    code = p.stderr.decode(errors='replace').strip().split('exit=')[-1]
    return p.stdout, int(code) if code.isdigit() else -1


# What the spec leaves out of the language, and jq's quirks of input the three refuse on purpose:
# a difference there is expected and only counted.
OUTSIDE = ['pow(', 'splits(', '$ENV', '@sh', '@html', '$__loc__', 'repeat(', 'flatten', 'label ', 'catch',
           '. as [', 'as [$a]', 'recurse(.[]?; ', 'leaf_paths', '$__prog_args', 'sort_by(.a; .b)']


def expected_difference(argv, stdin):
    text = ' '.join(argv)
    if any(o in text for o in OUTSIDE):
        return 'outside the subset'
    # Oniguruma's retry limit makes jq fail where SG's and Rust's regular expressions answer.
    if '(a+)+$' in text:
        return 'quirk of jq not followed'
    words = stdin.replace(b'[', b' ').replace(b']', b' ').replace(b',', b' ').split()
    for w in words:
        if w in (b'01', b'+1', b'.5', b'1.', b'nan', b'NaN', b'-nan', b'infinity', b'-infinity', b'-01', b'00',
                 b'"\\udc00"', b'"\\udc00"'):
            return 'quirk of jq not followed'
    if any(bytes([d]) + b'\x00' in stdin for d in b'0123456789'):
        return 'quirk of jq not followed'
    if 'fromjson' in text and b'nan' in stdin:
        return 'quirk of jq not followed'
    if b'\\udc00' in stdin and 'fromjson' not in text:
        return 'quirk of jq not followed'
    return None


failed = 0
passed = 0
expected = 0
for name, argv, stdin, timeout, shell in cases:
    if FILTER and FILTER not in name:
        continue
    if SMALL and (shell is not None or timeout > 60 or len(stdin) > 100000):
        continue
    try:
        want = run(JQ, argv, stdin, timeout, shell)
    except subprocess.TimeoutExpired:
        want = (b'<timeout>', -9)
    try:
        got = run(OURS, argv, stdin, timeout, shell)
    except subprocess.TimeoutExpired:
        got = (b'<timeout>', -9)
    if want == got:
        passed += 1
        if VERBOSE:
            print('ok  ', name[:120])
        continue
    why = expected_difference(argv, stdin)
    if why:
        expected += 1
        if VERBOSE:
            print('EXPECTED (' + why + ')', name[:120])
        continue
    failed += 1
    print('FAIL', name[:200], '| stdin:', stdin[:80])
    print('   jq  :', want[1], want[0][:300])
    print('   ours:', got[1], got[0][:300])
print(f'{passed} passed, {failed} failed, {expected} differ as expected (outside the subset or jq quirks)')
sys.exit(1 if failed else 0)
