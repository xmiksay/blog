// Sidebar.jsx — 2-level expandable left menu
function Sidebar({ activeId, onSelect }) {
  const [open, setOpen] = React.useState({ clanky: true, klub: true });
  const groups = [
    { id: 'klub', label: 'Klub', items: [
      { id: 'onas', label: 'O nás' },
      { id: 'clenove', label: 'Členové' },
      { id: 'historie', label: 'Historie' },
    ]},
    { id: 'hra', label: 'Hra', items: [
      { id: 'clanky', label: 'Články', children: [
        { id: 'rozbory', label: 'Rozbory partií' },
        { id: 'reporty', label: 'Turnajové reporty' },
      ]},
      { id: 'turnaje', label: 'Turnaje' },
      { id: 'treninky', label: 'Tréninky' },
    ]},
    { id: 'media', label: 'Média', items: [
      { id: 'galerie', label: 'Galerie' },
      { id: 'video', label: 'Videa' },
    ]},
  ];

  const toggle = (id) => setOpen(o => ({ ...o, [id]: !o[id] }));

  return (
    <aside className="bn-sidebar">
      {groups.map(g => (
        <div key={g.id}>
          <div className="bn-side-group">{g.label}</div>
          {g.items.map(it => (
            <React.Fragment key={it.id}>
              <div className={`bn-side-item ${activeId === it.id ? 'active' : ''} ${open[it.id] ? 'open' : ''}`}
                   onClick={() => { onSelect(it.id); if (it.children) toggle(it.id); }}>
                {it.label}
                {it.children && <span className="chev">▾</span>}
              </div>
              {it.children && open[it.id] && it.children.map(c => (
                <div key={c.id}
                     className={`bn-side-sub ${activeId === c.id ? 'active' : ''}`}
                     onClick={() => onSelect(c.id)}>{c.label}</div>
              ))}
            </React.Fragment>
          ))}
        </div>
      ))}
    </aside>
  );
}

window.Sidebar = Sidebar;
