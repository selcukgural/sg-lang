// The front page (choice 1384): what SG promises, three pillars, a program, and where it stands.
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
            <CodeBlock language="sg">{sample}</CodeBlock>
          </div>
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
        <section className={styles.pasha}>
          <img src="/img/pasha.png" alt="Pasha, the cat of SG" width="160" height="160" loading="lazy" />
          <p>Pasha, the cat of SG.</p>
        </section>
      </main>
    </Layout>
  );
}
