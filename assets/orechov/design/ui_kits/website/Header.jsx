// Header.jsx — top bar with knight glyph, nav, login
function Header({ activeTop, onTopChange }) {
  const items = [
    { id: 'clanky', label: 'Články' },
    { id: 'turnaje', label: 'Turnaje' },
    { id: 'onas', label: 'O nás' },
    { id: 'kontakt', label: 'Kontakt' },
  ];
  return (
    <header className="bn-header">
      <div className="bn-logo" onClick={() => onTopChange('clanky')}>
        <img src="../../assets/pieces/knight.svg" alt="" style={{ color: 'var(--ink)' }} />
        <div>
          <div className="wordmark">Bíločerný</div>
          <div className="sub">OŘECHOV · ŠACHOVÝ KLUB</div>
        </div>
      </div>
      <div className="bn-spacer" />
      <nav className="bn-topnav">
        {items.map(it => (
          <a key={it.id}
             className={activeTop === it.id ? 'active' : ''}
             onClick={() => onTopChange(it.id)}>{it.label}</a>
        ))}
      </nav>
      <button className="bn-login">Přihlásit</button>
    </header>
  );
}

window.Header = Header;
