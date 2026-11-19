// The front page (choices 1384 and 1404): what SG promises, three pillars, a program and what it prints, a
// mistake and what the compiler says of it, measurements against C, what was built with it, how to try it,
// and Pasha.
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
    var items = List<Item> { Item { name = "bolts", count = 40 }, Item { name = "nuts", count = 2 } }
    match total(read items) {
        Ok(sum) => print($"{sum} items in stock")
        Error(why) => print(why)
    }
}`;

// What the sample prints; `npm run check` runs it.
const sampleOutput = '42 items in stock';

// A mistake the checker refuses, and its message as `sg` writes it; `npm run check` compares the two.
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

// Time of SG divided by time of C, the same C compiler with -O2 (interpreter/README.md, "The C back end").
const measured = [
  ['Recursive `fib(40)`', '0.98'],
  ['A linked list of a million nodes, built, walked and freed', '1.01'],
  ['A list of a million numbers, filled, changed, added up', '1.02'],
  ['Three hundred million lookups that give back a borrow or nothing', '1.01'],
  ['Counting in a hash table, with a keyed hash against crafted keys', '1.20'],
  ['Printing two million structs', '0.57'],
];

const pillars = [
  {
    title: 'As fast and small as C',
    text: 'SG compiles through C, with no garbage collector and no runtime to carry. A check that safety does not need is left out by default; the checked way is one word away.',
  },
  {
    title: 'Memory safe like Rust',
    text: 'Ownership and borrows are checked when the program compiles. What depends on data, an index or an overflow, stops the program with a message that says what to write instead. `unsafe` marks the rest.',
  },
  {
    title: 'Written like C#',
    text: 'Types come first, blocks have braces and lines need no semicolons. `read` and `ref` say what a function does with what it is given. An error says what is wrong and what to write.',
  },
];

const built = [
  ['SQLite and libcurl', 'wrapped in modules whose users write no `unsafe`'],
  ['A small kernel', 'for RISC-V, booted in QEMU, with its devices driven in SG'],
  ['A queue without locks', 'for many producers and many consumers, with no `unsafe`'],
  ['A library for C', 'built from SG, called from a program of C through its header'],
];

export default function Home() {
  return (
    <Layout title="SG" description="SG: a small systems language, as fast and small as C, memory safe like Rust, written like C#.">
      <header className={styles.hero}>
        <div className={styles.heroInner}>
          <img className={styles.mark} src="/img/logo.svg" alt="" width="96" height="96" />
          <h1 className={styles.title}>SG</h1>
          <p className={styles.tagline}>A small systems language: as fast and small as C, memory safe like Rust, written like C#.</p>
          <div className={styles.buttons}>
            <Link className="button button--secondary button--lg" to="/docs/learn/tour">Take the tour</Link>
            <Link className="button button--outline button--secondary button--lg" to="/docs/learn/book/getting-started">Read the book</Link>
            <Link className="button button--outline button--secondary button--lg" to="/docs/learn/install">Install</Link>
          </div>
          <p className={styles.status}>
            Before its preview: the language and its tools are built and tested, and still change. <Link to="/docs/status">Where it stands</Link>
          </p>
        </div>
      </header>
      <main>
        <section className={styles.pillars}>
          {pillars.map((pillar) => (
            <div key={pillar.title} className={styles.pillar}>
              <h2>{pillar.title}</h2>
              <p><Words text={pillar.text} /></p>
            </div>
          ))}
        </section>
        <section className={styles.sample}>
          <div className={styles.sampleText}>
            <h2>What it looks like</h2>
            <p>
              <Words text="A function borrows the list it is given (`read`), gives a `Result`, and the caller looks at both ways. Nothing is copied, nothing is freed by hand, and the same program runs in the interpreter and compiled through C." />
            </p>
          </div>
          <div className={styles.sampleCode}>
            <CodeBlock language="sg" title="main.sg">{sample}</CodeBlock>
            <CodeBlock language="text" title="sg main.sg">{sampleOutput}</CodeBlock>
          </div>
        </section>
        <section className={styles.sample}>
          <div className={styles.sampleText}>
            <h2>What the compiler tells you</h2>
            <p>
              <Words text="A value has one owner. Here the list is given to `kept`, and `names` is used after it: the program does not compile, and the message shows both lines. A use after free, a borrow that outlives its value and an error left unread are found the same way, before the program runs." />
            </p>
            <p>
              <Link to="/docs/learn/book/getting-started">The book</Link> goes through the rules one at a time.
            </p>
          </div>
          <div className={styles.sampleCode}>
            <CodeBlock language="sg" title="main.sg">{mistake}</CodeBlock>
            <CodeBlock language="text" title="sg main.sg">{mistakeMessage}</CodeBlock>
          </div>
        </section>
        <section className={styles.measured}>
          <h2>Measured against C</h2>
          <p>
            <Words text="Each program is written twice, in SG and in C by hand, and both go through the same C compiler with `-O2`. The time of SG divided by the time of C; 1.00 is the same speed." />
          </p>
          <table>
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
          <p className={styles.small}>
            Small programs on two machines, macOS and Windows on Arm, not proof. With a plain hash the hash table is 0.99.{' '}
            <Link href="https://github.com/selcukgural/sg-lang/blob/main/interpreter/README.md#the-c-back-end">All the numbers, and how they were taken</Link>
          </p>
        </section>
        <section className={styles.built}>
          <h2>Built with it</h2>
          <ul>
            {built.map(([name, what]) => (
              <li key={name}>
                <strong>{name}</strong>: <Words text={what} />
              </li>
            ))}
          </ul>
        </section>
        <section className={styles.tryIt}>
          <div>
            <h2>Try it</h2>
            <p>
              <Words text="You need Rust to build `sg`, and a C compiler for compiled programs. The interpreter needs nothing more." />
            </p>
            <div className={styles.buttonsDark}>
              <Link className="button button--primary" to="/docs/learn/install">Install</Link>
              <Link className="button button--outline button--primary" to="/docs/learn/tour">Take the tour</Link>
            </div>
          </div>
          <CodeBlock language="bash">{`git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter
cargo build --release
target/release/sg examples/stack.sg`}</CodeBlock>
        </section>
        <section className={styles.pasha}>
          <img src="/img/pasha.png" alt="Pasha, a white Van cat with one blue and one amber eye, curled up in a circle" width="200" height="200" loading="lazy" />
          <p>
            <strong>Pasha</strong>, the cat of SG: a Van cat, one eye blue and one amber.
          </p>
        </section>
      </main>
    </Layout>
  );
}
