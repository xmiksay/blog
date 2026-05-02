// MarkdownArticle.jsx — renders a markdown article body via .bn-rich styles
// Uses the marked CDN parser so styles can be exercised against real MD.
function MarkdownArticle({ title, author, date, body }) {
  const html = React.useMemo(() => {
    if (typeof window.marked === 'undefined') return '';
    return window.marked.parse(body);
  }, [body]);

  return (
    <article>
      <div className="bn-breadcrumb"><a>Články</a> &nbsp;/&nbsp; <span>{title}</span></div>
      <header className="bn-masthead">
        <h1>{title}</h1>
        <div className="meta">{author} · {date}</div>
      </header>
      <div className="bn-rich" dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}

window.MarkdownArticle = MarkdownArticle;
