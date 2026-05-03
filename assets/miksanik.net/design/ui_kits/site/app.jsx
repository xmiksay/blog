// app.jsx — wires the kit together with fake routing.
const { useState, useEffect } = React;

const MENU = [
  { path: '/about', label: 'About' },
  { path: '/partie', label: 'Partie', children: [
    { path: '/partie/karjakin-carlsen-2016', label: 'Karjakin – Carlsen' },
    { path: '/partie/tata-steel-2024', label: 'Tata Steel 2024' }
  ]},
  { path: '/blog', label: 'Blog' },
  { path: '/notes', label: 'Notes' }
];

const POSTS = [
  { path: '/blog/pinning', date: '2025-08-12', tag: 'rust', title: 'Pinning a samoreference', summary: 'Proč Pin existuje a kdy ho potřebuješ — krátce a bez magie.' },
  { path: '/blog/cardano-validators', date: '2025-06-30', tag: 'cardano', title: 'Aiken validators v praxi', summary: 'Pár věcí, co bych si přál, aby mi někdo řekl dřív.' },
  { path: '/blog/portra-praha', date: '2025-04-02', tag: 'analog-foto', title: 'Portra 400, Praha v listopadu', summary: 'Skenuju, kontruju, učím se vidět.' },
  { path: '/partie/karjakin-carlsen-2016', date: '2024-11-22', tag: 'šachy', title: 'Karjakin – Carlsen, 2016', summary: 'Game 10 — když jeden tah obrátí celou matchovou strategii.' }
];

const SAMPLE_MOVES = [
  { san: 'start', fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR' },
  { san: '1. e4', fen: 'rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR', from: 52, to: 36 },
  { san: '1... c5', fen: 'rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR', from: 10, to: 26 },
  { san: '2. Nf3', fen: 'rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R', from: 62, to: 45 },
  { san: '2... d6', fen: 'rnbqkbnr/pp2pppp/3p4/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R', from: 11, to: 19 }
];

function HomePage({ onNav }) {
  return (
    <div>
      <h1 className="h1" style={{ marginBottom: 8 }}>Vítejte</h1>
      <p style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--fg-2)', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 24 }}>
        // martin mikšaník · personal blog &amp; znalostní báze
      </p>
      <p style={{ maxWidth: '60ch', marginBottom: 24 }}>
        Najdete tu poznámky k technologiím, které mě zajímají — Rust, Cardano, smart contracts, Linux — a občas něco navíc. Šachy, analogová fotografie, kytara.
      </p>
      <div className="article-list">
        {POSTS.map(p => (
          <article key={p.path}>
            <div className="meta">{p.date} · {p.tag}</div>
            <h2><a href="#" onClick={e => { e.preventDefault(); onNav(p.path); }}>{p.title}</a></h2>
            <p>{p.summary}</p>
          </article>
        ))}
      </div>
    </div>
  );
}

function ArticlePage({ post, onNav }) {
  return (
    <article className="article-detail">
      <div className="meta" style={{ marginBottom: 6 }}>
        <a href="#" onClick={e => { e.preventDefault(); onNav('/'); }} style={{ color: 'var(--fg-2)' }}>← zpět</a>
        {' · '}{post.date} · {post.tag}
      </div>
      <h1>{post.title}</h1>
      <div className="tag-list">
        <span className="tag active">{post.tag}</span>
        <span className="tag">notes</span>
        <span className="pill draft" style={{ marginLeft: 'auto' }}>DRAFT</span>
      </div>

      <div className="article-body">
        <p>{post.summary}</p>

        <h2>Setup</h2>
        <p>Než se dostaneme k jádru, krátký kontext. Tahle poznámka navazuje na:</p>

        <Transclude path="/notes/futures-101" summary="3 sections · what futures are, the executor contract, why polling.">
          <p><strong>Future</strong> je hodnota, která <em>možná ještě nemá výsledek</em>. Polled by an executor; returns <code>Poll::Ready(T)</code> or <code>Poll::Pending</code>.</p>
          <p>Kontrakt: jakmile vrátíš <code>Pending</code>, musíš se ujistit, že tě někdo později <code>wake()</code>-ne.</p>
        </Transclude>

        <h2>Příklad</h2>
        <pre>{`use std::pin::Pin;
use std::future::Future;

struct MyFut { state: u32 }

impl Future for MyFut {
    type Output = u32;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<u32> {
        Poll::Ready(self.state)
    }
}`}</pre>

        <h2>Šachová odbočka</h2>
        <p>Mimochodem, paralela: stejně jako <code>Pin</code> drží hodnotu na místě, ošacha drží své pole struktury — pokud ji rozbiješ, ztratíš tempo. Tady varianta z Karjakin–Carlsen, game 10:</p>

        <ChessViewer
          title="Karjakin – Carlsen, 2016"
          result="1–0"
          moves={SAMPLE_MOVES}
        />

        <h2>Galerie</h2>
        <p>Pár fotek z toho odpoledne — Portra 400, Pentax MX:</p>
        <Gallery title="Praha · listopad" items={[
          { title: 'Vltava', bg: 'linear-gradient(135deg,#0F151D,#1A2330)' },
          { title: 'Karlův most', bg: 'linear-gradient(160deg,#131A24,#0E141C)' },
          { title: 'Náplavka', bg: 'linear-gradient(110deg,#1A2330,#0F151D)' },
          { title: 'Tramvaj', bg: 'linear-gradient(135deg,#0E141C,#1A2330)' },
          { title: 'Mosty', bg: 'linear-gradient(135deg,#1A2330,#131A24)' },
          { title: 'Letná', bg: 'linear-gradient(135deg,#131A24,#1A2330)' }
        ]} />

        <ArticleImage title="35mm · Portra 400 · 1/250 f/4" />

        <blockquote>"Make it work, make it right, make it fast." — and don't fear <code>Pin</code>.</blockquote>
      </div>
    </article>
  );
}

function EditorPage() {
  const [saved, setSaved] = useState(null);
  return (
    <div>
      <h1 className="h1" style={{ marginBottom: 8 }}>Editor</h1>
      <p style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--fg-2)', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 16 }}>
        // admin · markdown · directives
      </p>
      <Editor
        initial={`## Pinning a samoreference

::page{path=/notes/futures-101}

\`Pin<P>\` znamená, že hodnota za **P** se v paměti nepohne.

::pgn{src=karjakin-carlsen-2016.pgn}

::gallery{path=/photos/2024-praha}
`}
        onSave={(v) => setSaved(v.length)}
      />
      {saved !== null && (
        <div style={{ marginTop: 12, fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--acc-green)' }}>
          ✓ saved · {saved} chars
        </div>
      )}
    </div>
  );
}

function App() {
  const [path, setPath] = useState('/');
  const [scanKey, setScanKey] = useState(0);

  function nav(p) {
    setPath(p);
    setScanKey(k => k + 1);
  }

  const post = POSTS.find(p => p.path === path);

  return (
    <div className="layout">
      <Sidebar menu={MENU} currentPath={path} onNav={nav} loggedIn={false} />
      <div className="content">
        <main>
          <span key={scanKey} className="scanline active"></span>
          {path === '/' && <HomePage onNav={nav} />}
          {path === '/admin' && <EditorPage />}
          {post && <ArticlePage post={post} onNav={nav} />}
          {!post && path !== '/' && path !== '/admin' && (
            <div>
              <h1 className="h1">{path}</h1>
              <p style={{ color: 'var(--fg-2)', fontFamily: 'var(--font-mono)' }}>// stub · this route would render server-side via Tera</p>
              <p style={{ marginTop: 12 }}><a href="#" onClick={e => { e.preventDefault(); nav('/'); }}>← zpět</a></p>
            </div>
          )}
        </main>
        <footer>© miksanik.net · <a href="#">Sitemap</a> · v0.4.2</footer>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
