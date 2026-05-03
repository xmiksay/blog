// Editor.jsx — admin markdown editor matching md-toolbar.js + md-preview.js behavior
function Editor({ initial = '', onSave }) {
  const [value, setValue] = React.useState(initial);
  const [showPreview, setShowPreview] = React.useState(false);
  const taRef = React.useRef(null);

  function wrap(prefix, suffix) {
    const ta = taRef.current;
    if (!ta) return;
    const s = ta.selectionStart, e = ta.selectionEnd;
    const sel = value.substring(s, e);
    const next = value.substring(0, s) + prefix + sel + suffix + value.substring(e);
    setValue(next);
    setTimeout(() => {
      ta.focus();
      ta.selectionStart = s + prefix.length;
      ta.selectionEnd = s + prefix.length + sel.length;
    }, 0);
  }

  const buttons = [
    ['B', '**', '**'], ['I', '_', '_'],
    ['H2', '## ', ''], ['H3', '### ', ''],
    ['Link', '[', '](url)'], ['Img', '[img ', ']'], ['Code', '`', '`']
  ];

  return (
    <div className="editor-wrapper">
      <div className="md-toolbar">
        {buttons.map(([l, p, s]) => (
          <button key={l} type="button" onClick={() => wrap(p, s)}>{l}</button>
        ))}
        <span className="spacer"></span>
        <button type="button" className="preview-toggle" onClick={() => setShowPreview(v => !v)}>
          {showPreview ? 'Hide preview' : 'Preview'}
        </button>
        {onSave && <button type="button" onClick={() => onSave(value)} style={{ color: 'var(--acc-green)', borderColor: 'var(--acc-green-bd)', background: 'var(--acc-green-bg)' }}>Save</button>}
      </div>
      <textarea ref={taRef} id="markdown" value={value} onChange={e => setValue(e.target.value)} />
      {showPreview && (
        <div className="md-preview" style={{ display: 'block' }}>
          <pre style={{ background: 'transparent', border: 0, padding: 0, color: 'var(--fg-2)', fontSize: 11, marginBottom: 8 }}>// preview · directives rendered server-side</pre>
          <div style={{ whiteSpace: 'pre-wrap', fontFamily: 'var(--font-body)', color: 'var(--fg-1)', fontSize: 14 }}>{value}</div>
        </div>
      )}
    </div>
  );
}
window.Editor = Editor;
