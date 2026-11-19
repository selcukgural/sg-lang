// Docusaurus's loader of Prism languages, with SG added (choice 1388): the same words and marks
// as the TextMate grammar of the VS Code extension (editors/vscode/syntaxes/sg.tmLanguage.json),
// so code looks the same on the site and in the editor.
import siteConfig from '@generated/docusaurus.config';

const control = 'if|else|while|for|in|return|break|continue|match|try|await|lock|when|with|is|not|or|as|unsafe';
const declaration = 'var|const|static|struct|enum|union|using|use|extern|export|async|test|assert|interface|embed|packed|align';
const modifier = 'public|readonly|read|ref|shared|weak|checked|locked|atomic|raw|share|new';
const primitive = 'void|bool|char|string|sbyte|byte|short|ushort|int|uint|long|ulong|nint|nuint|float|double';

function defineSg(Prism) {
  const sg = {
    comment: /\/\/.*/,
    'raw-string': { pattern: /\$?"""[\s\S]*?"""/, greedy: true, alias: 'string' },
    'interpolated-string': {
      pattern: /\$"(?:\\.|\{\{|\}\}|\{[^{}"]*\}|[^\\"{}\n])*"/,
      greedy: true,
      inside: {
        interpolation: {
          pattern: /\{(?!\{)[^{}]*\}/,
          inside: {
            punctuation: /^\{|\}$/,
            'format-spec': { pattern: /:[A-Za-z]\d*$/, alias: 'constant' },
            rest: null,
          },
        },
        string: /[\s\S]+/,
      },
    },
    string: { pattern: /"(?:\\.|[^\\"\n])*"/, greedy: true },
    char: { pattern: /'(?:\\(?:[ntr0\\"']|u\{[0-9A-Fa-f]{1,6}\})|[^'\\\n])'/, alias: 'string' },
    keyword: new RegExp(`\\b(?:${control}|${declaration}|${modifier})\\b`),
    builtin: new RegExp(`\\b(?:${primitive})\\b`),
    boolean: /\b(?:true|false)\b/,
    constant: /\b(?:null|this)\b/,
    'class-name': /\b[A-Z][A-Za-z0-9_]*\b/,
    function: /\b[a-z_][A-Za-z0-9_]*(?=\s*\()/,
    number: /\b(?:0x[\dA-Fa-f_]+|0b[01_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)\b/,
    operator: /=>|\?\?|\?\.|\.\.=?|&&|\|\||==|!=|<=|>=|<<=?|>>=?|[-+*\/%&|^~!<>=]=?/,
    punctuation: /[{}[\]();,.:]/,
  };
  sg['interpolated-string'].inside.interpolation.inside.rest = sg;
  Prism.languages.sg = sg;
}

export default function prismIncludeLanguages(PrismObject) {
  const {
    themeConfig: { prism },
  } = siteConfig;
  const { additionalLanguages } = prism;
  const PrismBefore = globalThis.Prism;
  globalThis.Prism = PrismObject;
  additionalLanguages.forEach((lang) => {
    // eslint-disable-next-line global-require, import/no-dynamic-require
    require(`prismjs/components/prism-${lang}`);
  });
  defineSg(PrismObject);
  delete globalThis.Prism;
  if (typeof PrismBefore !== 'undefined') {
    globalThis.Prism = PrismObject;
  }
}
