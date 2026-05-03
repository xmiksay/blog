// Gallery.jsx — ::gallery and ::img
function Gallery({ title, items = [] }) {
  return (
    <div className="gallery">
      {title && <h3 style={{ fontFamily: 'var(--font-display)', fontSize: 16, marginBottom: 8 }}>{title}</h3>}
      <div className="gallery-grid">
        {items.map((it, i) => (
          <a key={i} href="#" className="gallery-item" onClick={e => e.preventDefault()}>
            <div style={{
              width: '100%', aspectRatio: 1,
              background: it.bg || `linear-gradient(${135 + i * 17}deg, #0F151D, #1A2330)`,
              borderRadius: 'var(--r-1)', border: '1px solid var(--line-1)'
            }} />
            <span>{it.title}</span>
          </a>
        ))}
      </div>
    </div>
  );
}

function ArticleImage({ title, alt, bg }) {
  return (
    <figure className="article-image">
      <a href="#" onClick={e => e.preventDefault()}>
        <div style={{
          width: '100%', height: 220,
          background: bg || 'linear-gradient(135deg, #0F151D, #1A2330)',
          borderRadius: 'var(--r-2)', border: '1px solid var(--line-1)'
        }} />
      </a>
      <figcaption>{title}</figcaption>
    </figure>
  );
}

window.Gallery = Gallery;
window.ArticleImage = ArticleImage;
