# markdown-it-compiler

Easy configurable markdown-it compiler

## Installation

```bash
yarn add markdown-it-compiler
```

## Usage

Instantiate an instance of the compiler and sequentially compile markdown to html.

```js
const config = {...};
const compiler = new MarkdownItCompiler(config);
const doc = compiler.compile(mardownString);
const content = doc.html;
```

The config as follows:

- **preset** - markdown-it preset name (default: _'default'_)
- **options** - options for `markdown-it`, see
  [docs](https://github.com/markdown-it/markdown-it#init-with-presets-and-options)
- **plugins** - array of plugins to `use()` (see below)
- **configure** - detailed configuration of markdown-it. Type: `(md: MarkdownIt) => void`
- **format** - postprocess the html. Type: `(content: Content) => string` (see
  below)

Example:

````js
const config = {
  options: {
    linkify: true,
    html: true,
    typographer: true
  },
  plugins: [
    'markdown-it-abbr',
    'markdown-it-anchor',
    'markdown-it-deflist',
    'markdown-it-highlightjs',
    'markdown-it-ins',
    'markdown-it-mark',
    [
      'markdown-it-plantuml',
      {
        openMarker: '```plantuml\n@startuml',
        closeMarker: '@enduml\n```'
      }
    ],
    'markdown-it-sub',
    'markdown-it-sup'
  ],
  configure(md) {
    // load custom plugins
    md.use(require('./lib/my-md-it-plugin'));
  },
  format(content) {
    if (content.attributes.layout && content.attributes.layout === 'article') {
      return `<article>${content.html}</article>`;
    }

    return content.html;
  }
};
````

### Post-Processing

Use the `format` key for postprocessing. The passed `content` has the following
structure:

- **attributes** - attributes from frontmatter
- **toc** - structured table of contents, see
  [markdown-toc](https://github.com/jonschlinkert/markdown-toc)
- **body** - the original markdown body
- **html** - the already generated html
