// ChessViewer.jsx — visual shell for ::pgn / ::fen.
// Real engine (chess.js + chessboard.js) drops in unchanged via /static/js/chess-viewer.js.
function ChessViewer({ pgn, title, result, moves = [], initialMove = 0 }) {
  const [moveIdx, setMoveIdx] = React.useState(initialMove);
  const totalMoves = moves.length;
  const board = React.useMemo(() => buildBoard(moves[moveIdx]?.fen), [moveIdx, moves]);

  return (
    <div className="pgn-viewer">
      <div className="board">
        {board.map((piece, i) => {
          const r = Math.floor(i / 8), c = i % 8;
          const isHl = moves[moveIdx]?.from === i || moves[moveIdx]?.to === i;
          return (
            <div key={i} className={"sq " + ((r + c) % 2 === 0 ? 'l' : 'd') + (isHl ? ' hl' : '')}>
              {piece ? <img src={`../../assets/pieces/${piece}.png`} alt={piece} /> : null}
            </div>
          );
        })}
      </div>
      <div className="controls">
        <button onClick={() => setMoveIdx(0)}>⏮</button>
        <button onClick={() => setMoveIdx(i => Math.max(0, i - 1))}>◀</button>
        <span className="move-info">{moves[moveIdx]?.san || (totalMoves ? `${moveIdx + 1}/${totalMoves}` : 'start')}</span>
        <button onClick={() => setMoveIdx(i => Math.min(totalMoves - 1, i + 1))}>▶</button>
        <button onClick={() => setMoveIdx(totalMoves - 1)}>⏭</button>
      </div>
      {(title || result) && (
        <div style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--fg-2)', textAlign: 'center', marginTop: 6 }}>
          {title}{result ? ` · ${result}` : ''}
        </div>
      )}
    </div>
  );
}

// Helper: tiny piece placement renderer. Real component uses chess.js to derive FEN per move.
function buildBoard(fen) {
  const start = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR';
  const rows = (fen || start).split(' ')[0].split('/');
  const out = [];
  for (const row of rows) {
    for (const ch of row) {
      if (/\d/.test(ch)) for (let i = 0; i < +ch; i++) out.push(null);
      else out.push((ch === ch.toUpperCase() ? 'w' : 'b') + ch.toUpperCase());
    }
  }
  return out;
}

window.ChessViewer = ChessViewer;
