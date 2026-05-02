// ArticleList.jsx — listing for /clanky
function ArticleList({ items, onOpen }) {
  return (
    <>
      <header className="bn-masthead">
        <h1>Články</h1>
        <div className="meta">PŘÍSPĚVKY KLUBU · NEJNOVĚJŠÍ NAHOŘE</div>
      </header>
      <div className="bn-list">
        {items.map(a => (
          <div key={a.id} className="bn-article-card" onClick={() => onOpen(a.id)}>
            <h2><a>{a.title}</a></h2>
            <div className="meta">{a.author} · {a.date}</div>
            <p>{a.excerpt}</p>
          </div>
        ))}
      </div>
    </>
  );
}

window.ArticleList = ArticleList;
