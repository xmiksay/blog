// Tournaments.jsx — event grid
function Tournaments({ events }) {
  return (
    <>
      <header className="bn-masthead">
        <h1>Turnaje</h1>
        <div className="meta">NADCHÁZEJÍCÍ AKCE · JARO 2026</div>
      </header>
      <div className="bn-events">
        {events.map(e => (
          <div key={e.id} className="bn-event">
            <div className="date">
              <div className="day">{e.day}</div>
              <div className="mon">{e.mon}</div>
            </div>
            <div>
              <h3>{e.title}</h3>
              <div className="where">{e.where}</div>
              <p>{e.desc}</p>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

window.Tournaments = Tournaments;
