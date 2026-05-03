// Sidebar.jsx — recreates the Tera macro from assets/common/templates/base.html
function Sidebar({ menu, currentPath, onNav, loggedIn }) {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <a href="#" className="logo" onClick={(e) => { e.preventDefault(); onNav('/'); }}>
          <img src="logo.png" alt="" className="logo-mark" />
          <span className="logo-text">miksanik<span className="tld">.net</span></span>
        </a>
      </div>
      <div className="sidebar-body">
        <nav>
          <ul>
            <li>
              <div className={"nav-row" + (currentPath === '/search' ? ' active' : '')}>
                <a href="#" onClick={(e) => { e.preventDefault(); onNav('/search'); }}>Search</a>
              </div>
            </li>
            {menu.map(item => (
              <li key={item.path} className={item.children ? 'has-children' : ''}>
                <div className={"nav-row" + (currentPath === item.path ? ' active' : '')}>
                  <a href="#" onClick={(e) => { e.preventDefault(); onNav(item.path); }}>{item.label}</a>
                  {item.children && <span className="nav-caret" aria-hidden="true"></span>}
                </div>
                {item.children && (
                  <ul>
                    {item.children.map(c => (
                      <li key={c.path}>
                        <div className={"nav-row" + (currentPath === c.path ? ' active' : '')}>
                          <a href="#" onClick={(e) => { e.preventDefault(); onNav(c.path); }}>{c.label}</a>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        </nav>
        <div className="sidebar-bottom">
          <span style={{ color: 'var(--fg-3)' }}>// status</span>
          <a href="#" onClick={(e) => { e.preventDefault(); onNav('/admin'); }}>
            {loggedIn ? '$ admin' : '$ login'}
          </a>
        </div>
      </div>
    </aside>
  );
}

window.Sidebar = Sidebar;
