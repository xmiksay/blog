// Transclude.jsx — ::page{path=...} component
function Transclude({ path, children, summary }) {
  const [collapsed, setCollapsed] = React.useState(false);
  return (
    <div className={"transclude" + (collapsed ? ' collapsed' : '')}>
      <div className="tx-head" onClick={() => setCollapsed(c => !c)}>
        <span>{collapsed ? '▸' : '▾'}</span>
        <span className="path">{path}</span>
        <span className="toggle">{collapsed ? 'expand' : 'collapse'}</span>
      </div>
      {collapsed
        ? <div className="summary">{summary || 'collapsed · click to expand'}</div>
        : <div className="tx-body">{children}</div>}
    </div>
  );
}
window.Transclude = Transclude;
