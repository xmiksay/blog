import { Chess } from './chess.js';

// --- Static position (FEN) ---
document.querySelectorAll('.fen-viewer').forEach((el, idx) => {
    const boardId = 'fen-board-' + idx;
    const div = document.createElement('div');
    div.id = boardId;
    el.appendChild(div);

    Chessboard(boardId, {
        position: el.dataset.fen,
        draggable: false,
        pieceTheme: '/static/img/chesspieces/wikipedia/{piece}.png',
    });
});

// --- PGN viewer with move playback ---
document.querySelectorAll('.pgn-viewer').forEach((el, idx) => {
    const chess = new Chess();
    chess.loadPgn(el.dataset.pgn);

    const history = chess.history({ verbose: true });
    let current = -1;

    // Determine starting move: data-move="last" (default), data-move="first", or data-move="N"
    const moveAttr = el.dataset.move;
    let startAt;
    if (moveAttr === 'first' || moveAttr === '0') {
        startAt = -1;
    } else if (moveAttr && moveAttr !== 'last') {
        startAt = Math.min(parseInt(moveAttr, 10) - 1, history.length - 1);
    } else {
        startAt = history.length - 1; // default: final position
    }

    chess.reset();

    const boardId = 'pgn-board-' + idx;
    const boardDiv = el.querySelector('.board');
    boardDiv.id = boardId;

    const board = Chessboard(boardId, {
        position: chess.fen(),
        draggable: false,
        pieceTheme: '/static/img/chesspieces/wikipedia/{piece}.png',
    });
    const info = el.querySelector('.move-info');

    const show = () => {
        info.textContent = current < 0
            ? 'Start position'
            : `${Math.floor(current / 2) + 1}${current % 2 === 0 ? '.' : '…'} ${history[current].san}`;
    };

    const goTo = i => {
        if (i < 0) {
            chess.reset();
            board.position(chess.fen(), true);
            current = -1;
            show();
            return;
        }
        chess.reset();
        for (let j = 0; j <= i; j++) chess.move(history[j]);
        board.position(chess.fen(), true);
        current = i;
        show();
    };

    el.querySelector('.btn-first').onclick = () => goTo(-1);
    el.querySelector('.btn-prev').onclick = () => current > -1 && goTo(current - 1);
    el.querySelector('.btn-next').onclick = () => current < history.length - 1 && goTo(current + 1);
    el.querySelector('.btn-last').onclick = () => goTo(history.length - 1);

    // Show initial position
    goTo(startAt);
});
