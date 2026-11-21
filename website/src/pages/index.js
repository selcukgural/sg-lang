// The front page (choices 1384, 1404 and 1405), laid out as the pages of Rust and Zig are: what SG is in one
// sentence beside a program that runs, why it exists, what the compiler catches, what it is built for, how
// fast it is, and where to go next.
import React from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import CodeBlock from '@theme/CodeBlock';
import styles from './index.module.css';

// Text with `code` in backticks, as the docs write it.
function Words({ text }) {
  return text.split('`').map((part, at) => (at % 2 === 1 ? <code key={at}>{part}</code> : <React.Fragment key={at}>{part}</React.Fragment>));
}

// A program that runs: the interpreter and its compiled build print `42 items in stock`.
const sample = `struct Item {
    string name
    int count
}

Result<int, string> total(read List<Item> items) {
    var sum = 0
    for item in items {
        if item.count < 0 {
            return Error($"{item.name} has a negative count".toString())
        }
        sum += item.count
    }
    return Ok(sum)
}

void main() {
    var items = List<Item> {
        Item { name = "bolts", count = 40 },
        Item { name = "nuts", count = 2 },
    }
    match total(read items) {
        Ok(sum) => print($"{sum} items in stock")
        Error(why) => print(why)
    }
}`;

// What the sample prints; `npm run check` runs it.
const sampleOutput = '42 items in stock';

// Two mistakes the checker refuses, and their messages as `sg` writes them; `npm run check` compares them.
const mistake = `void main() {
    var names = List<string> { "Ada", "Grace" }
    var kept = names
    print(names.length())
    print(kept.length())
}`;

const mistakeMessage = `error: \`names\` was moved and can no longer be used
  --> main.sg:4:11
 4 |     print(names.length())
   |           ^^^^^
  it was moved here:
  --> main.sg:3:16
 3 |     var kept = names
   |                ^^^^^`;

const dangle = `read string longest(read List<string> names) {
    var best = names[0].clone()
    for name in read names {
        if name.length() > best.length() {
            best = name.clone()
        }
    }
    return read best
}

void main() {
    var names = List<string> { "Ada", "Grace" }
    print(longest(read names))
}`;

const dangleMessage = `error: this returns a borrow of \`best\`, and \`best\` is gone when the function returns
  --> main.sg:8:12
 8 |     return read best
   |            ^^^^^^^^^
  hint: return the value itself instead of a borrow of it`;

const pillars = [
  {
    title: 'Memory safe',
    text: 'Ownership and borrows are checked when the program compiles, with no garbage collector. What depends on data, an index or an overflow, stops the program with a message that says what to write instead. `unsafe` marks the rest.',
  },
  {
    title: 'As fast as C',
    text: 'SG compiles through C, with no runtime to carry. A check that safety does not need is left out by default; the checked way is one word away. Every claim is measured against C, Rust and Zig.',
  },
  {
    title: 'Small and plain',
    text: 'Types come first, blocks have braces and lines need no semicolons. `read` and `ref` say what a function does with what it is given. A feature has to pay for itself, at run time and in what it costs to learn.',
  },
];

const mistakes = [
  {
    title: 'A value used after it was given away',
    text: 'A value has one owner. Here the list is given to `kept`, and `names` is used after it: the message shows both lines.',
    code: mistake,
    message: mistakeMessage,
  },
  {
    title: 'A borrow that outlives its value',
    text: 'A function lends back `best`, which ends when the function returns. In C this is a dangling pointer; here it does not compile, and the hint says what to write.',
    code: dangle,
    message: dangleMessage,
  },
];

const uses = [
  {
    title: 'Command-line tools',
    text: 'Arguments and options, files and folders, other programs, regular expressions checked when the program compiles, JSON and a log: in the library, measured against Rust and Zig.',
    to: '/docs/library/command-line',
    link: 'The command line',
  },
  {
    title: 'Network services',
    text: 'Tasks and `async`/`await` on one pool of threads, TCP and UDP, cancellation, channels and locks. A task is always waited for, and the thread rule is checked when the program compiles.',
    to: '/docs/learn/book/concurrency',
    link: 'Tasks and async',
  },
  {
    title: 'Next to C',
    text: 'Call C and be called by it, lay out its types as C does, and ship a library of SG with a header. SQLite, libcurl and zlib are wrapped in modules whose users write no `unsafe`.',
    to: '/docs/learn/book/c',
    link: 'Calling C',
  },
  {
    title: 'Without an operating system',
    text: 'A program built `--freestanding` runs on the machine itself: a small kernel for RISC-V boots in QEMU with its devices driven in SG, and atomics follow the orders of C11.',
    to: '/docs/learn/book/building#without-an-operating-system',
    link: 'Building programs',
  },
];

// Time of SG divided by time of C, the same C compiler with -O2 (benchmarks/against-c.md).
const measured = [
  ['Recursive `fib(40)`', '0.98'],
  ['A linked list of a million nodes, built, walked and freed', '1.01'],
  ['A list of a million numbers, filled, changed, added up', '1.02'],
  ['Three hundred million lookups that give back a borrow or nothing', '1.01'],
  ['Counting in a hash table, with a keyed hash against crafted keys', '1.20'],
  ['Printing two million structs', '0.47'],
];

const figures = [
  ['61', 'programs written in SG and in C by hand'],
  ['50', 'of them in the time of C within 5%, or faster'],
  ['100 MB', 'of JSON read, walked and written faster than `serde_json` and Zig'],
];

const next = [
  {
    title: 'Learn',
    links: [
      ['A tour of SG', '/docs/learn/tour'],
      ['The SG book', '/docs/learn/book/getting-started'],
      ['SG for Rust programmers', '/docs/learn/for-rust'],
      ['The library', '/docs/library'],
    ],
  },
  {
    title: 'Follow',
    links: [
      ['Where it stands', '/docs/status'],
      ['Why SG', '/docs/about/why'],
      ['How it got here', '/docs/about/history'],
      ['Benchmarks', '/docs/benchmarks'],
    ],
  },
  {
    title: 'Take part',
    links: [
      ['The source on GitHub', 'https://github.com/selcukgural/sg-lang'],
      ['Every decision, with its reason', 'https://github.com/selcukgural/sg-lang/blob/main/design/rules/README.md'],
      ['The repository', '/docs/about/repository'],
      ['Reporting a security issue', 'https://github.com/selcukgural/sg-lang/blob/main/SECURITY.md'],
    ],
  },
];

export default function Home() {
  return (
    <Layout title="SG" description="SG, a small systems language: safe by design, fast by default.">
      <header className={styles.hero}>
        <div className={styles.heroInner}>
          <div className={styles.heroText}>
            <div className={styles.brand}>
              <img className={styles.mark} src="/img/logo-hero.svg" alt="" width="88" height="88" />
              <span className={styles.title}>SG</span>
            </div>
            <p className={styles.tagline}>Safe by design. Fast by default.</p>
            <h1 className={styles.lead}>
              A small systems language: as fast as C, memory safe like Rust, and written the way C# reads.
            </h1>
            <div className={styles.buttons}>
              <Link className="button button--secondary button--lg" to="/docs/learn/tour">Get started</Link>
              <Link className="button button--outline button--secondary button--lg" to="/docs/learn/install">Install</Link>
            </div>
            <p className={styles.status}>
              <span className={styles.badge}>Before preview</span> Built and tested on Linux, macOS and Windows; it still changes.{' '}
              <Link to="/docs/status">Where it stands</Link>
            </p>
          </div>
          <div className={styles.heroCode}>
            <CodeBlock language="sg" title="main.sg">{sample}</CodeBlock>
            <CodeBlock language="text" title="sg main.sg">{sampleOutput}</CodeBlock>
          </div>
        </div>
      </header>
      <main>
        <section className={styles.section}>
          <h2 className={styles.heading}>Why SG?</h2>
          <div className={styles.cards}>
            {pillars.map((pillar) => (
              <div key={pillar.title} className={styles.card}>
                <h3>{pillar.title}</h3>
                <p><Words text={pillar.text} /></p>
              </div>
            ))}
          </div>
        </section>
        <section className={styles.band}>
          <div className={styles.section}>
            <h2 className={styles.heading}>Caught before the program runs</h2>
            <p className={styles.intro}>
              <Words text="A use after free, a borrow that outlives its value and an error left unread are compile errors in SG, each with the line that caused it and, when it is known, the fix." />
            </p>
            <div className={styles.mistakes}>
              {mistakes.map((item) => (
                <div key={item.title} className={styles.mistake}>
                  <h3>{item.title}</h3>
                  <p><Words text={item.text} /></p>
                  <CodeBlock language="sg" title="main.sg">{item.code}</CodeBlock>
                  <CodeBlock language="text" title="sg main.sg">{item.message}</CodeBlock>
                </div>
              ))}
            </div>
            <p className={styles.more}>
              <Link to="/docs/learn/book/ownership">The book</Link> goes through the rules one at a time.
            </p>
          </div>
        </section>
        <section className={styles.section}>
          <h2 className={styles.heading}>Build it in SG</h2>
          <div className={styles.uses}>
            {uses.map((use) => (
              <div key={use.title} className={styles.card}>
                <h3>{use.title}</h3>
                <p><Words text={use.text} /></p>
                <Link to={use.to}>{use.link} →</Link>
              </div>
            ))}
          </div>
        </section>
        <section className={styles.band}>
          <div className={styles.section}>
            <h2 className={styles.heading}>Measured, not promised</h2>
            <div className={styles.figures}>
              {figures.map(([figure, what]) => (
                <div key={figure} className={styles.figure}>
                  <span className={styles.number}>{figure}</span>
                  <span><Words text={what} /></span>
                </div>
              ))}
            </div>
            <div className={styles.pair}>
              <div>
                <p>
                  <Words text="Each program is written twice, in SG and in C by hand, and both go through the same C compiler with `-O2`. Real programs and every module of the library are measured the same way against Rust and Zig, and a case SG loses is published too." />
                </p>
                <p className={styles.small}>
                  Small programs on two machines, macOS and Windows on Arm, not proof. With a plain hash the hash table is 0.99.
                </p>
                <Link className="button button--primary" to="/docs/benchmarks">All the numbers</Link>
              </div>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th>Program</th>
                    <th>SG / C</th>
                  </tr>
                </thead>
                <tbody>
                  {measured.map(([program, ratio]) => (
                    <tr key={program}>
                      <td><Words text={program} /></td>
                      <td className={styles.ratio}>{ratio}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>
        <section className={styles.section}>
          <h2 className={styles.heading}>Try it</h2>
          <div className={styles.tryIt}>
            <div>
              <p>
                <Words text="You need Rust to build `sg`, and a C compiler for compiled programs. The interpreter needs nothing more." />
              </p>
              <div className={styles.buttonsInline}>
                <Link className="button button--primary" to="/docs/learn/install">Install</Link>
                <Link className="button button--outline button--primary" to="/docs/learn/tour">Take the tour</Link>
              </div>
            </div>
            <CodeBlock language="bash">{`git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter
cargo build --release
target/release/sg examples/stack.sg`}</CodeBlock>
          </div>
          <div className={styles.next}>
            {next.map((column) => (
              <div key={column.title}>
                <h3>{column.title}</h3>
                <ul>
                  {column.links.map(([label, to]) => (
                    <li key={label}>
                      <Link to={to}>{label}</Link>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </section>
        <section className={styles.pasha}>
          <img src="/img/pasha.png" alt="Pasha, a white Van cat with one blue and one amber eye, curled up in a circle" width="88" height="88" loading="lazy" />
          <p>
            <strong>Pasha</strong>, the cat of SG: a Van cat, one eye blue and one amber.
          </p>
        </section>
      </main>
    </Layout>
  );
}
