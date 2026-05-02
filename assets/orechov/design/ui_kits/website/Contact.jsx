// Contact.jsx — two-column contact page
function Contact() {
  return (
    <>
      <header className="bn-masthead">
        <h1>Kontakt</h1>
        <div className="meta">SPOJTE SE S KLUBEM</div>
      </header>
      <div className="bn-twocol">
        <dl className="bn-info-list">
          <dt>Adresa</dt><dd>Komenského 702/4, 664 44 Ořechov</dd>
          <dt>E-mail</dt><dd><a href="#">info@orechov.cz</a></dd>
          <dt>IČO</dt><dd>21 12 34 56</dd>
          <dt>Tréninky</dt><dd>Středa 18:00 · Sokolovna Ořechov</dd>
          <dt>Předseda</dt><dd>Miksa Novák</dd>
        </dl>
        <form className="bn-form" onSubmit={(e) => e.preventDefault()}>
          <label>Jméno<input defaultValue="" placeholder="Jan Novák" /></label>
          <label>E-mail<input type="email" placeholder="jan@example.cz" /></label>
          <label>Zpráva<textarea rows="5" placeholder="Mám dotaz ohledně tréninků…"></textarea></label>
          <button type="submit">Odeslat zprávu</button>
        </form>
      </div>
    </>
  );
}

window.Contact = Contact;
