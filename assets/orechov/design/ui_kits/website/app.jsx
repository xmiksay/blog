// app.jsx — assembles UI kit screens
const SAMPLE_MD = `# Šachový klub Bíločerný Ořechov

**Bíločerný Ořechov, z.s.** je šachový klub působící v obci Ořechov nedaleko Brna. Spolek byl oficiálně založen *15. dubna 2025*.

## Základní informace

Sídlo klubu se nachází na adrese:

- Komenského 702/4, 664 44 Ořechov
- IČO: 21 12 34 56
- Předseda: Miksa Novák

Klub vznikl s cílem rozvíjet šachovou komunitu v regionu a navázat na tradici šachového života v okolí Brna.

> Šachy učí trpělivosti a úctě k soupeři — to je hodnota, kterou chceme předávat dál.
> <cite>Z výroční zprávy 2025</cite>

## Poslání a činnost

Hlavním účelem spolku je:

1. organizace a rozvoj šachové hry
2. vytváření podmínek pro trénink a soutěže
3. podpora mládeže a začátečníků
4. reprezentace obce Ořechov v soutěžích

---

## Rozbor zahájení

Italská hra je klasickým otevřeným zahájením, vhodným pro začátečníky:

\`\`\`
1. e4 e5
2. Nf3 Nc6
3. Bc4 Bc5
\`\`\`

Pozice po třetím tahu nabízí vyrovnanou hru s rychlým vývinem lehkých figur. Více v článku [Italská hra v praxi](#).

## Tabulka výsledků jaro 2026

| Kolo | Soupeř           | Výsledek | Body |
|------|------------------|----------|------|
| 1.   | Lokomotiva Brno  | 4½ : 3½  | 2    |
| 2.   | Duras Brno B     | 3 : 5    | 0    |
| 3.   | Sokol Židlochovice | 5 : 3  | 2    |
| 4.   | Šlapanice        | 4 : 4    | 1    |

## Užitečné odkazy

- [Šachový svaz ČR](#)
- [Krajský přebor JmK](#)
- \`info@orechov.cz\` — kontaktujte nás kdykoli
`;

const ARTICLES = [
  { id: 'rozbor', title: 'Rozbor partie: Italská hra', author: 'Miksa', date: '8. 4. 2026',
    excerpt: 'Klasické zahájení rozebrané tah po tahu — od základů po typické střední hry.' },
  { id: 'turnaj-report', title: 'Turnajový report: Jaro v Brně', author: 'Miksa', date: '2. 4. 2026',
    excerpt: 'Naše první sezóna v krajském přeboru družstev. Co fungovalo, co zlepšit.' },
  { id: 'onas-art', title: 'O nás', author: 'Miksa', date: '8. 4. 2025',
    excerpt: 'Bíločerný Ořechov, z.s. je nový šachový klub působící v obci Ořechov.' },
];

const EVENTS = [
  { id: 'starosty', day: '17', mon: 'KVĚ', title: 'Turnaj o pohár starosty',
    where: 'SOKOLOVNA OŘECHOV · 10:00', desc: 'Otevřený turnaj 7 kol švýcarským systémem, tempo 15 + 5.' },
  { id: 'mladez', day: '24', mon: 'KVĚ', title: 'Mládežnický turnaj',
    where: 'KLUBOVNA · 14:00', desc: 'Pro děti do 14 let. Občerstvení a ceny pro všechny účastníky.' },
  { id: 'kraj', day: '07', mon: 'ČVN', title: 'Krajský přebor družstev',
    where: 'BRNO · CELÝ DEN', desc: 'Závěrečné kolo soutěže družstev. Sestava bude oznámena týden předem.' },
  { id: 'simu', day: '21', mon: 'ČVN', title: 'Simultánka s mistrem',
    where: 'KAVÁRNA U KRÁLE · 18:00', desc: 'IM Petr Bartoň hraje simultánně proti 20 hráčům klubu.' },
];

function App() {
  const [active, setActive] = React.useState('clanky');
  const [openArticle, setOpenArticle] = React.useState(null);

  const handleTop = (id) => { setActive(id); setOpenArticle(null); };
  const handleSide = (id) => {
    setActive(id);
    if (id === 'rozbory' || id === 'reporty') { setOpenArticle('rozbor'); }
    else { setOpenArticle(null); }
  };

  let view;
  if (openArticle) {
    const a = ARTICLES.find(x => x.id === openArticle) || ARTICLES[0];
    view = <MarkdownArticle title={a.title} author={a.author} date={a.date} body={SAMPLE_MD} />;
  } else if (active === 'turnaje') {
    view = <Tournaments events={EVENTS} />;
  } else if (active === 'kontakt') {
    view = <Contact />;
  } else if (active === 'onas' || active === 'onas-art') {
    view = <MarkdownArticle title="O nás" author="Miksa" date="8. 4. 2025" body={SAMPLE_MD} />;
  } else {
    view = <ArticleList items={ARTICLES} onOpen={(id) => setOpenArticle(id)} />;
  }

  // map sidebar id from top selection
  const sideId = openArticle ? 'rozbory' : (active === 'clanky' ? 'clanky' : active);

  return (
    <div className="app">
      <Header activeTop={active === 'clanky' || openArticle ? 'clanky' : active} onTopChange={handleTop} />
      <div className="bn-shell">
        <Sidebar activeId={sideId} onSelect={handleSide} />
        <main className="bn-content">
          <div className="bn-content-inner">
            {view}
          </div>
          <Footer />
        </main>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
