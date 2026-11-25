#!/usr/bin/env python3
"""Lox programs made from a seed, run by clox and by each build, which must write the same bytes and
end with the same code (choice 1888). Run from the repository's root:

    python3 -I trials/lox/tests/fuzz.py [--count N] [--seed S] [--keep DIR] BINARY...

A program declares globals, functions (closures among them, recursion bounded by an argument),
classes with fields, methods, initializers, inheritance and `super`, and runs loops bounded by a
counter, printing as it goes. Its expressions mix numbers, strings, `nil`, booleans and objects, so
that some of them are runtime errors (an operand of the wrong type, a property of a number, a call of
a string, too few arguments); one program in eight is cut or has a token changed, so that the
compiler reports errors. Each build named is also run with `--stress-gc` and `--ip-local`. A program
on which a build differs is written into `--keep` (default `trials/target/lox/fuzz`); one that clox does
not end in ten seconds is left out and counted. `clock()` is only compared with 0: clox's is the
processor's time, SG's the time since the program started.
"""

import argparse
import random
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))
import clox  # noqa: E402

NUMBERS = ["0", "1", "2", "3", "10", "0.5", "1.25", "100", "123456789", "0.1", "7.75", "1000000"]
WORDS = ["a", "b", "ab", "x", "hello", "", "lox", "zz"]


class Gen:
    def __init__(self, rng):
        self.r = rng
        self.lines = []
        self.globals = []
        self.functions = []  # (name, arity)
        self.classes = []  # (name, methods {name: arity}, fields)
        self.fresh = 0

    def name(self, prefix):
        self.fresh += 1
        return f"{prefix}{self.fresh}"

    def literal(self):
        k = self.r.random()
        if k < 0.45:
            return self.r.choice(NUMBERS)
        if k < 0.75:
            return '"' + self.r.choice(WORDS) + '"'
        if k < 0.85:
            return self.r.choice(["true", "false"])
        return "nil"

    def number(self, depth):
        if depth > 3 or self.r.random() < 0.5:
            return self.r.choice(NUMBERS)
        return f"({self.number(depth + 1)} {self.r.choice(['+', '-', '*', '/'])} {self.number(depth + 1)})"

    def text(self, depth):
        if depth > 3 or self.r.random() < 0.6:
            return '"' + self.r.choice(WORDS) + '"'
        return f"({self.text(depth + 1)} + {self.text(depth + 1)})"

    def expr(self, scope, depth=0):
        r = self.r.random()
        if depth > 3 or r < 0.3:
            if scope and self.r.random() < 0.6:
                return self.r.choice(scope)
            return self.literal()
        if r < 0.55:
            op = self.r.choice(["+", "+", "-", "*", "/", "<", ">", "<=", ">=", "==", "!="])
            # Mostly operands of the type the operator takes, so that most programs run on.
            if self.r.random() < 0.85:
                if op == "+" and self.r.random() < 0.4:
                    return f"({self.text(depth + 1)} + {self.text(depth + 1)})"
                if op not in ("==", "!="):
                    return f"({self.number(depth + 1)} {op} {self.number(depth + 1)})"
            return f"({self.expr(scope, depth + 1)} {op} {self.expr(scope, depth + 1)})"
        if r < 0.62:
            return f"{self.r.choice(['-', '!'])}{self.expr(scope, depth + 1)}"
        if r < 0.68:
            return f"({self.expr(scope, depth + 1)} {self.r.choice(['and', 'or'])} {self.expr(scope, depth + 1)})"
        if r < 0.8 and self.functions:
            name, arity = self.r.choice(self.functions)
            if self.r.random() < 0.05:
                arity += self.r.choice([-1, 1]) if arity else 1
            args = ", ".join(self.expr(scope, depth + 1) for _ in range(arity))
            return f"{name}({args})"
        if r < 0.92 and self.classes:
            cname, methods, fields = self.r.choice(self.classes)
            obj = f"{cname}({', '.join(self.expr(scope, depth + 1) for _ in range(methods.get('init', 0)))})"
            k = self.r.random()
            if k < 0.4 and fields:
                return f"{obj}.{self.r.choice(fields)}"
            if k < 0.8 and methods:
                m = self.r.choice(list(methods))
                return f"{obj}.{m}({', '.join(self.expr(scope, depth + 1) for _ in range(methods[m]))})"
            return obj
        if r < 0.95:
            return "(clock() < 0)"
        return f"{self.literal()}.{self.r.choice(['f', 'g'])}"

    def block(self, scope, depth, budget, in_function):
        out = []
        local = list(scope)
        for _ in range(self.r.randint(1, budget)):
            out += self.statement(local, depth, in_function)
        return out

    def statement(self, scope, depth, in_function):
        r = self.r.random()
        ind = "  " * depth
        if r < 0.3:
            return [f"{ind}print {self.expr(scope)};"]
        if r < 0.45:
            v = self.name("v")
            scope.append(v)
            return [f"{ind}var {v} = {self.expr(scope[:-1])};"]
        if r < 0.55 and scope:
            # The value assigned reads no variable, so that no text doubles itself in a loop.
            return [f"{ind}{self.r.choice(scope)} = {self.expr([])};"]
        if r < 0.65 and depth < 4:
            return [f"{ind}if ({self.expr(scope)}) {{"] + self.block(scope, depth + 1, 3, in_function) + \
                   [f"{ind}}} else {{"] + self.block(scope, depth + 1, 2, in_function) + [f"{ind}}}"]
        if r < 0.75 and depth < 4:
            i = self.name("i")
            n = self.r.randint(0, 6)
            return [f"{ind}for (var {i} = 0; {i} < {n}; {i} = {i} + 1) {{"] + \
                self.block(scope + [i], depth + 1, 3, in_function) + [f"{ind}}}"]
        if r < 0.8 and depth < 4:
            c = self.name("w")
            return [f"{ind}var {c} = 0;", f"{ind}while ({c} < {self.r.randint(0, 5)}) {{",
                    f"{ind}  {c} = {c} + 1;"] + self.block(scope + [c], depth + 1, 3, in_function) + [f"{ind}}}"]
        if r < 0.85 and in_function:
            return [f"{ind}return {self.expr(scope)};"]
        if r < 0.9 and depth < 3:
            return [f"{ind}{{"] + self.block(scope, depth + 1, 3, in_function) + [f"{ind}}}"]
        return [f"{ind}{self.expr(scope)};"]

    def function(self, depth=0):
        name = self.name("f")
        arity = self.r.randint(0, 3)
        params = [self.name("p") for _ in range(arity)]
        lines = [f"fun {name}({', '.join(params)}) {{"]
        # A guard keeps recursion bounded: the first parameter, or a global count.
        lines += [f"  if (calls > 200) return nil;", "  calls = calls + 1;"]
        lines += self.block(params, 1, 5, True)
        if self.r.random() < 0.5:
            # A closure over a parameter or a local, returned or called.
            inner = self.name("g")
            v = self.name("c")
            lines += [f"  var {v} = {self.expr(params)};", f"  fun {inner}() {{ {v} = {v}; return {v}; }}",
                      f"  return {inner};" if self.r.random() < 0.5 else f"  return {inner}();"]
        lines.append("}")
        self.functions.append((name, arity))
        return lines

    def klass(self):
        name = self.name("K")
        parent = self.r.choice(self.classes) if self.classes and self.r.random() < 0.5 else None
        methods = dict(parent[1]) if parent else {}
        fields = list(parent[2]) if parent else []
        lines = [f"class {name}" + (f" < {parent[0]}" if parent else "") + " {"]
        if self.r.random() < 0.7:
            arity = self.r.randint(0, 2)
            params = [self.name("p") for _ in range(arity)]
            body = []
            for p in params:
                f = self.name("fd")
                fields.append(f)
                body.append(f"    this.{f} = {p};")
            if parent and "init" in parent[1] and self.r.random() < 0.7:
                body.insert(0, f"    super.init({', '.join(self.literal() for _ in range(parent[1]['init']))});")
            lines += [f"  init({', '.join(params)}) {{"] + body + ["  }"]
            methods["init"] = arity
        for _ in range(self.r.randint(0, 3)):
            m = self.name("m")
            arity = self.r.randint(0, 2)
            params = [self.name("p") for _ in range(arity)]
            body = ["    calls = calls + 1;", "    if (calls > 200) return nil;"]
            body += ["    " + s for s in self.block(params, 0, 3, True)]
            if fields and self.r.random() < 0.6:
                body.append(f"    return this.{self.r.choice(fields)};")
            if parent and parent[1] and self.r.random() < 0.4:
                pm = self.r.choice([k for k in parent[1] if k != "init"] or ["init"])
                if pm != "init":
                    body.append(f"    return super.{pm}({', '.join(self.literal() for _ in range(parent[1][pm]))});")
            lines += [f"  {m}({', '.join(params)}) {{"] + body + ["  }"]
            methods[m] = arity
        lines.append("}")
        self.classes.append((name, methods, fields))
        return lines

    def program(self):
        out = ["var calls = 0;"]
        for _ in range(self.r.randint(2, 8)):
            k = self.r.random()
            if k < 0.35:
                out += self.function()
            elif k < 0.6:
                out += self.klass()
            else:
                out += self.statement(self.globals, 0, False)
        out += self.block(self.globals, 0, 12, False)
        return "\n".join(out) + "\n"


def damage(rng, text):
    """A program cut short, or with one token changed into another."""
    if rng.random() < 0.5:
        return text[: rng.randint(0, len(text))]
    words = text.split(" ")
    at = rng.randrange(len(words))
    words[at] = rng.choice(["", ";", "(", ")", "{", "}", "var", "class", "=", ".", "super", "this", "return", "\"", "@"])
    return " ".join(words)


def run(cmd, path, timeout):
    try:
        r = subprocess.run(cmd + [str(path)], capture_output=True, timeout=timeout)
        return r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired:
        return b"", b"timed out", "timeout"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=2000)
    parser.add_argument("--seed", type=int, default=181)
    parser.add_argument("--keep", default=str(ROOT / "trials" / "target" / "lox" / "fuzz"))
    parser.add_argument("builds", nargs="+")
    args = parser.parse_args()
    reference = str(clox.build())
    keep = Path(args.keep)
    keep.mkdir(parents=True, exist_ok=True)
    work = keep / "current.lox"
    runs = []
    for b in args.builds:
        runs += [[b], [b, "--stress-gc"], [b, "--ip-local"]]
    differ = 0
    kinds = {0: 0, 65: 0, 70: 0}
    for n in range(args.count):
        rng = random.Random(args.seed * 1_000_003 + n)
        text = Gen(rng).program()
        if rng.random() < 0.125:
            text = damage(rng, text)
        work.write_text(text)
        want = run([reference], work, 10)
        if want[2] == "timeout":
            # A program clox does not end in ten seconds is not held to it.
            kinds["skipped"] = kinds.get("skipped", 0) + 1
            continue
        kinds[want[2]] = kinds.get(want[2], 0) + 1
        for cmd in runs:
            got = run(cmd, work, 120)
            if got != want:
                differ += 1
                kept = keep / f"differs-{args.seed}-{n}.lox"
                kept.write_text(text)
                print(f"{' '.join(cmd)} differs on {kept}: exit {got[2]} != {want[2]}")
                break
    print(f"{args.count} programs (ended 0: {kinds.get(0, 0)}, compile errors: {kinds.get(65, 0)}, "
          f"runtime errors: {kinds.get(70, 0)}, longer than 10 s in clox: {kinds.get('skipped', 0)}), {differ} differ")
    sys.exit(1 if differ else 0)


if __name__ == "__main__":
    main()
