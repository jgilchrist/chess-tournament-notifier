use super::tcec::EngineName;
use anyhow::{bail, Result};
use pgn_reader::{BufferedReader, RawComment, RawHeader, SanPlus, Skip, Visitor};
use std::hash::{Hash, Hasher};

const EVENT_KEY: &str = "Event";
const WHITE_HEADER_KEY: &str = "White";
const BLACK_HEADER_KEY: &str = "Black";
const DATE_HEADER_KEY: &str = "Date";
const BOOK_MOVE_COMMENT_PREFIX: &str = "book,";

#[derive(Debug, Clone)]
pub struct PgnMove {
    notation: String,
    in_book: bool,
}

#[derive(Debug, Clone)]
pub struct Pgn {
    pub white_player: EngineName,
    pub black_player: EngineName,
    pub date: String,
    pub event: String,

    pub moves: Vec<PgnMove>,
}

impl Pgn {
    // There's a strange issue where moves later in the game can be reported as 'book' moves.
    // It seems to happen for tablebase moves or other moves with no UCI info.
    // To ensure we get only the opening, stop once we hit the first non-book move.
    fn opening(&self) -> impl Iterator<Item = &PgnMove> {
        self.moves.iter().take_while(|mv| mv.in_book)
    }

    /// The game is 'out of book' if any of the moves that were played are not book moves
    pub fn out_of_book(&self) -> bool {
        self.moves.iter().any(|mv| !mv.in_book)
    }

    pub fn has_player(&self, player: &str) -> bool {
        self.white_player_is(player) || self.black_player_is(player)
    }

    fn white_player_is(&self, player: &str) -> bool {
        self.white_player.matches(player)
    }

    fn black_player_is(&self, player: &str) -> bool {
        self.black_player.matches(player)
    }

    pub fn as_hash(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

// The hash of a TCEC PGN is the hash of the players, the date, and the book.
// That is to say, we consider games equivalent if they are played by the same players
// on the same day, with the same opening book.
// FIXME: This doesn't account for replays.
impl Hash for Pgn {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.white_player.hash(state);
        self.black_player.hash(state);
        self.date.hash(state);

        for mv in self.opening() {
            mv.notation.hash(state);
        }
    }
}

impl PartialEq<Self> for Pgn {
    fn eq(&self, other: &Self) -> bool {
        self.as_hash() == other.as_hash()
    }
}

impl Eq for Pgn {}

struct PgnInfoBuilder {
    pub white_player: Option<String>,
    pub black_player: Option<String>,
    pub date: Option<String>,
    pub event: Option<String>,

    pub moves: Vec<PgnMove>,

    pub last_san: Option<String>,
    pub last_comment: Option<String>,
}

impl PgnInfoBuilder {
    pub fn new() -> PgnInfoBuilder {
        Self {
            white_player: None,
            black_player: None,
            date: None,
            event: None,
            moves: vec![],

            last_san: None,
            last_comment: None,
        }
    }
}

impl PgnInfoBuilder {
    pub fn add_move(&mut self, san: &str, comment: &str) {
        let is_book_move = comment.starts_with(BOOK_MOVE_COMMENT_PREFIX);

        self.moves.push(PgnMove {
            notation: san.to_owned(),
            in_book: is_book_move,
        });
    }
}

impl Visitor for PgnInfoBuilder {
    type Result = Pgn;

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        let key = String::from_utf8_lossy(key);
        let value = value.decode_utf8_lossy();

        if key == EVENT_KEY {
            self.event = Some(value.to_string());
        }

        if key == WHITE_HEADER_KEY {
            self.white_player = Some(value.to_string());
        }

        if key == BLACK_HEADER_KEY {
            self.black_player = Some(value.to_string());
        }

        if key == DATE_HEADER_KEY {
            self.date = Some(value.to_string());
        }
    }

    fn san(&mut self, san: SanPlus) {
        if let Some(last_san) = self.last_san.clone() {
            self.add_move(&last_san, &self.last_comment.clone().unwrap_or(String::new()))
        }

        self.last_comment = None;
        self.last_san = Some(san.to_string());
    }

    fn comment(&mut self, comment: RawComment<'_>) {
        let comment = String::from_utf8_lossy(comment.as_bytes()).to_string();
        self.last_comment = Some(comment);
    }

    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {
        // Handle the last move we saw
        if let Some(last_san) = self.last_san.clone() {
            self.add_move(&last_san, &self.last_comment.clone().unwrap_or(String::new()))
        }

        assert_ne!(self.white_player, None);
        assert_ne!(self.black_player, None);
        assert_ne!(self.date, None);
        assert_ne!(self.event, None);

        Pgn {
            white_player: EngineName::new(&self.white_player.clone().unwrap()),
            black_player: EngineName::new(&self.black_player.clone().unwrap()),
            date: self.date.clone().unwrap(),
            event: self.event.clone().unwrap(),
            moves: self.moves.clone(),
        }
    }
}

pub fn get_pgn_info(pgn: &str) -> Result<Pgn> {
    let mut reader = BufferedReader::new_cursor(pgn);

    let pgn_info = reader.read_game(&mut PgnInfoBuilder::new())?;

    let Some(pgn_info) = pgn_info else {
        bail!("Empty PGN")
    };

    Ok(pgn_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgn_parsing_grabs_correct_information() {
        let sample_pgn = r#"[Event "TCEC Season 29 - Category 1 Playoff"]
[Site "https://tcec-chess.com"]
[Date "2025.12.02"]
[Round "2.1"]
[White "c4ke 1.1"]
[Black "Minic 3.44"]
[Result "*"]
[BlackElo "3436"]
[ECO "B43"]
[GameStartTime "2025-12-02T13:20:38.758 UTC"]
[Opening "Sicilian"]
[Termination "unterminated"]
[TimeControl "1800+3"]
[Variation "Kan, 5.Nc3"]
[WhiteElo "3183"]

{WhiteEngineOptions: Protocol=uci; Threads=256; Hash=262144;, BlackEngineOptions: Protocol=uci; Threads=512; Hash=256000; PawnHash=2048; NNUEFile=embedded; CommandLineOptions=-uci -syzygyPath /home/syzygy7;}
1. e4 {book, mb=+0+0+0+0+0,} c5 {book, mb=+0+0+0+0+0,}
2. Nf3 {book, mb=+0+0+0+0+0,} e6 {book, mb=+0+0+0+0+0,}
3. d4 {book, mb=+0+0+0+0+0,} cxd4 {book, mb=-1+0+0+0+0,}
4. Nxd4 {book, mb=+0+0+0+0+0,} a6 {book, mb=+0+0+0+0+0,}
5. Nc3 {book, mb=+0+0+0+0+0,} Qc7 {book, mb=+0+0+0+0+0,}
6. g3 {book, mb=+0+0+0+0+0,} b5 {book, mb=+0+0+0+0+0,}
7. Bg2 {d=32, sd=32, mt=96132, tl=1706868, s=0, n=0, pv=Bg2, tb=null, h=0.0, ph=0.0, wv=0.74, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Nc6 {d=33, sd=52, mt=126033, tl=1676967, s=192424652, n=24238964382, pv=Bb7 O-O Be7 Re1 d6 a4 b4 Na2 Nf6 Nxb4 O-O Nd3 Nbd7 Bd2 Rac8 Qe2 Ne5 a5 Rfd8 Bc3 Nc4 Bb4 Nd7 c3 Bf8 Red1 Re8 Nb3 Nde5 Nxe5, tb=1, h=36.6, ph=0.0, wv=0.88, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
8. O-O {d=34, sd=34, mt=108197, tl=1601671, s=0, n=0, pv=O-O, tb=null, h=0.0, ph=0.0, wv=0.68, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Nxd4 {d=35, sd=53, pd=O-O, mt=150055, tl=1529912, s=197324099, n=29601378978, pv=Nxd4 Qxd4 f6 Be3 Ne7 a4 b4 Qxb4 Rb8 Qd4 Nc6 Qd1 h5 b3 h4 Ne2 g5 Nc1 Bb4 Rb1 a5 Qe2 d6 h3 Bc5 Nd3 Bxe3 Qxe3 hxg3 fxg3 O-O Rfc1 Ne5 Nxe5, tb=10, h=71.1, ph=100.0, wv=0.92, R50=50, Rd=-9, Rr=-1000, mb=+0-1+0+0+0,}
9. Qxd4 {d=35, sd=35, mt=100682, tl=1503989, s=0, n=0, pv=Qxd4, tb=null, h=0.0, ph=0.0, wv=1.14, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
f6 {d=33, sd=48, pd=Qxd4, mt=53840, tl=1479072, s=208728575, n=11231475551, pv=Ne7 a4 b4 Qxb4 Rb8 Qd4 Nc6 Qc4 Rb4 Qe2 Be7 Re1 Bf6 Bf4 d6 Nd1 Bxb2 Nxb2 Rxb2 Bc1 Rb6 Be3 Rb8 Red1 Na5 Bf1 O-O Qd3 Rd8 Rab1 h5 Rdc1 Rxb1 Rxb1, tb=4, h=83.5, ph=100.0, wv=1.10, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
10. Rd1 {d=34, sd=34, mt=83264, tl=1423725, s=0, n=0, pv=Rd1, tb=null, h=0.0, ph=0.0, wv=0.54, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Bb7 {d=36, sd=51, pd=Be3, mt=117012, tl=1365060, s=201592485, n=23581280709, pv=Bb7 Be3 Ne7 a4 Nc6 Qd2 b4 Na2 Ne5 Bf4 a5 c3 bxc3 Nxc3 Bb4 Nb5 Qb6 Be3 Qd8 Qe2 O-O f4 Nf7 Qd3 d6 Rdc1 Rc8 Rxc8 Bxc8 Rc1 Bd7 Qb3 g5 fxg5 fxg5 Nc7 Bc5 Bxc5 Qxc7, tb=20, h=89.9, ph=66.6, wv=0.67, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
11. Be3 {d=36, sd=36, mt=106534, tl=1320191, s=0, n=0, pv=Be3, tb=null, h=0.0, ph=0.0, wv=1.01, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Ne7 {d=36, sd=61, pd=Be3, mt=144513, tl=1223547, s=200444379, n=28958802352, pv=Ne7 a4 Nc6 Qd2 b4 Na2 Be7 Bf4 Qd8 Bd6 Bxd6 Qxd6 Qe7 Qc7 d6 Qxd6 Rd8 Qxe7+ Kxe7 e5 a5 exf6+ gxf6 b3 Rxd1+ Rxd1 Rc8 h3 Rc7 c3 bxc3 Nxc3 Ne5 Rc1 Bxg2 Kxg2 Rc5 Ne2 Rxc1 Nxc1 Kd6 f4 Nd7 Kf3 Kc5 f5 exf5 Kf4 Kd4 Kxf5 Kc3 Ke6 Nc5+ Kxf6 Ne4+, tb=150, h=95.2, ph=75.0, wv=0.84, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
12. Qd2 {d=36, sd=36, mt=84680, tl=1238511, s=0, n=0, pv=Qd2, tb=null, h=0.0, ph=0.0, wv=1.08, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Nc6 {d=35, sd=57, pd=Qd2, mt=115271, tl=1111276, s=202816783, n=23370374394, pv=Nc6 a4 b4 Na2 Be7 b3 Rd8 Rac1 Qa5 c3 bxc3 Rxc3 Ba8 Qc2 O-O Rcd3 Rc8 Rxd7 Ne5 Qd2 Qxd2 R7xd2 a5 Bd4 Nd7 Rc1 Ba3 Rb1 Nc5 e5 fxe5 Bxe5 Be4 Bxe4 Nxe4 Re2 Bc5 Rxe4 Rxf2, tb=255, h=97.1, ph=80.0, wv=0.66, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
13. a4 {d=36, sd=36, mt=63665, tl=1177846, s=0, n=0, pv=a4, tb=null, h=0.0, ph=0.0, wv=1.44, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
b4 {d=35, sd=58, pd=a4, mt=90024, tl=1024252, s=205445730, n=18490115719, pv=b4 Na2 Be7 Bf4 Qd8 Bd6 Bxd6 Qxd6 Qe7 Qc7 d6 Qxd6 Rd8 Qxe7+ Kxe7 e5 a5 exf6+ gxf6 b3 Rxd1+ Rxd1 Rc8 Nc1 Nd8 Rd2 Bxg2 Kxg2 Nf7 f3 h5 h3 e5 Re2 Ke6 Nd3 Kf5 g4+ Kg5 Nf2 hxg4 hxg4 f5 gxf5 Kxf5 Nd3 Rc3, tb=194, h=98.6, ph=83.3, wv=0.85, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
14. Na2 {d=34, sd=34, mt=80976, tl=1099870, s=0, n=0, pv=Na2, tb=null, h=0.0, ph=0.0, wv=0.87, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Be7 {d=32, sd=59, pd=Na2, mt=60913, tl=966339, s=210473095, n=12813180747, pv=Be7 Bf4 Qd8 Bd6 Bxd6 Qxd6 Qe7 Qc7 d6 Qxd6 Rd8 Qxe7+ Kxe7 e5 a5 exf6+ gxf6 b3 Rxd1+ Rxd1 Rc8 Nc1 Nd8 Rd2 Bxg2 Kxg2 Nf7 Nd3 Nd6 Nf4 Rc6 Kf3 e5 Nd5+ Ke6 Ne3 Rc5 g4 Rc3 Kg2, tb=202, h=98.9, ph=85.7, wv=0.76, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
15. Bf4 {d=37, sd=37, mt=62145, tl=1040725, s=0, n=0, pv=Bf4, tb=null, h=0.0, ph=0.0, wv=1.45, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Qd8 {d=36, sd=56, pd=Bf4, mt=128000, tl=841339, s=208638123, n=20096858070, pv=Qd8 Bd6 a5 Rac1 Bxd6 f4 Qe7 Rb1 Rc8 Qxd6 Qxd6 Rxd6 Ke7 c3 Kxd6 cxb4, tb=2669, h=99.2, ph=87.5, wv=0.74, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
16. Bd6 {d=39, sd=39, mt=54745, tl=988980, s=0, n=0, pv=Bd6, tb=null, h=0.0, ph=0.0, wv=1.47, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Bxd6 {d=38, sd=59, pd=Bd6, mt=93051, tl=751288, s=209291026, n=19466995541, pv=Bxd6 Qxd6 Qe7 Qc7 d6 Qxd6 Rd8 Qxe7+ Kxe7 e5 Rb8 exf6+ gxf6 Rd2 Ne5 Re1 Nf3+ Bxf3 Bxf3 Re3 Bc6 b3 Rhd8 Rxd8 Rxd8 Kf1 a5 Ke1 e5 Nc1 Bd5 Nd3 h5 Nb2 Rg8 f3 Rc8 Kd2 Ke6 f4 e4 Nc4 Bxc4 Rxe4+ Kd5 Rxc4 Rxc4 bxc4+, tb=24526, h=99.6, ph=88.8, wv=0.88, R50=50, Rd=-9, Rr=-1000, mb=+0+0-1+0+0,}
17. Qxd6 {d=42, sd=42, mt=79375, tl=912605, s=0, n=0, pv=Qxd6, tb=null, h=0.0, ph=0.0, wv=1.38, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Qe7 {d=35, sd=55, pd=Qxd6, mt=41546, tl=712742, s=232587351, n=9656562004, pv=Qe7 Qc7 d6 Qxd6 Rd8 Qxe7+ Kxe7 e5 Rb8 exf6+ gxf6 f4 Rhd8 Rxd8 Nxd8 Rd1 Bxg2 Kxg2 b3 cxb3 Rxb3 Rd2 Nb7 Nc1 Rb4 b3 e5 fxe5 fxe5 Kf2 Na5 Rd3 Ke6 Rc3 Kd6 h3 Kd5 Rd3+ Ke6 Re3 Kd6, tb=19595, h=99.7, ph=90.0, wv=0.76, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
*
"#;

        let pgn_info = get_pgn_info(sample_pgn).unwrap();

        assert!(pgn_info.white_player.matches("c4ke"));
        assert!(pgn_info.black_player.matches("Minic"));
        assert_eq!(pgn_info.date, "2025.12.02");
        assert_eq!(pgn_info.event, "TCEC Season 29 - Category 1 Playoff");
        assert!(pgn_info.out_of_book())
    }

    #[test]
    fn test_pgn_parsing_in_book_returns_true() {
        let sample_pgn = r#"[Event "TCEC Season 29 - Category 1 Playoff"]
[Site "https://tcec-chess.com"]
[Date "2025.12.02"]
[Round "2.1"]
[White "c4ke 1.1"]
[Black "Minic 3.44"]
[Result "*"]
[BlackElo "3436"]
[ECO "B43"]
[GameStartTime "2025-12-02T13:20:38.758 UTC"]
[Opening "Sicilian"]
[Termination "unterminated"]
[TimeControl "1800+3"]
[Variation "Kan, 5.Nc3"]
[WhiteElo "3183"]

{WhiteEngineOptions: Protocol=uci; Threads=256; Hash=262144;, BlackEngineOptions: Protocol=uci; Threads=512; Hash=256000; PawnHash=2048; NNUEFile=embedded; CommandLineOptions=-uci -syzygyPath /home/syzygy7;}
1. e4 {book, mb=+0+0+0+0+0,} c5 {book, mb=+0+0+0+0+0,}
2. Nf3 {book, mb=+0+0+0+0+0,} e6 {book, mb=+0+0+0+0+0,}
3. d4 {book, mb=+0+0+0+0+0,} cxd4 {book, mb=-1+0+0+0+0,}
4. Nxd4 {book, mb=+0+0+0+0+0,} a6 {book, mb=+0+0+0+0+0,}
5. Nc3 {book, mb=+0+0+0+0+0,} Qc7 {book, mb=+0+0+0+0+0,}
*
"#;

        let pgn_info = get_pgn_info(sample_pgn).unwrap();
        assert!(!pgn_info.out_of_book())
    }

    #[test]
    fn test_pgn_parsing_does_not_panic_for_moves_with_no_comment() {
        let sample_pgn = r#"[Event "TCEC Season 29 - Category 1 Playoff"]
[Site "https://tcec-chess.com"]
[Date "2025.12.02"]
[Round "2.4"]
[White "Sirius 54101d91"]
[Black "Winter 4.02c"]
[Result "*"]
[BlackElo "3427"]
[ECO "B06"]
[GameStartTime "2025-12-02T16:34:14.733 UTC"]
[Opening "Robatsch (modern) defence"]
[Termination "unterminated"]
[TimeControl "1800+3"]
[WhiteElo "3396"]

{WhiteEngineOptions: Protocol=uci; Threads=512; Hash=262144;, BlackEngineOptions: Protocol=uci; Threads=256; Hash=65536; OwnBook=false; Ponder=false;}
1. e4 {book, mb=+0+0+0+0+0,} g6 {book, mb=+0+0+0+0+0,}
2. d4 {book, mb=+0+0+0+0+0,} Bg7 {book, mb=+0+0+0+0+0,}
3. Nf3 {book, mb=+0+0+0+0+0,} c5 {book, mb=+0+0+0+0+0,}
4. c3 {book, mb=+0+0+0+0+0,} cxd4 {book, mb=-1+0+0+0+0,}
5. cxd4 {book, mb=+0+0+0+0+0,} Nc6 {book, mb=+0+0+0+0+0,}
6. Nc3 {d=43, sd=69, mt=57349, tl=1745651, s=268884090, n=15419964817, pv=Nc3 d6 d5 Ne5 Nxe5 Bxe5 f4 Bg7 Be3 a6 Be2 Nf6 O-O O-O a4 Nd7 Rb1 Nf6 h3 Bd7 Bf3 b5 axb5 Bxb5 Re1 Nd7 Qd2 Qb8 Rec1 Rc8 Nxb5 axb5 Rxc8+ Qxc8 Rc1 Qb8 Qc2 Qd8 Kh2 h5 e5 dxe5 d6 exd6 Bxa8 Qxa8 Qc8+ Qxc8 Rxc8+ Kh7 Rc7 exf4 Bxf4 Ne5 Rb7 Nd3 Bxd6 Bxb2, tb=null, h=44.5, ph=0.0, wv=0.60, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
d6 {d=32, sd=94, mt=72563, tl=1730437, s=144131382, n=10457596597, pv=d6 h3 e6 Bb5 Ne7 O-O O-O Re1 h6 Be3 d5 e5 Bd7 Bd3 f6 exf6 Rxf6 Rc1 Nf5 Bxf5 Rxf5 Ne2 Qf8 Qb3 b6 Nh4 Rf6 f4 Rc8 Nf3 Qe8 Bd2 Rf8 a3 Qf7 Kh2 Qf5 Ng3 Qf7 Qe3 Kh7 b3 Kg8 a4 Qe7 Ne2 Kh7 Qd3 a5 Ne5 Qe8 Rf1 Bf6 Qe3 Bg7 g4 Kg8 Kg2 Nxe5 dxe5 b5 axb5 Bxb5 Rxc8 Qxc8 Rc1 Qa6 Nd4 Qb6 Nxb5 Qxb5 Rc5, tb=null, h=84.7, ph=0.0, wv=0.81, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
7. d5 {d=44, sd=79, mt=133712, tl=1614939, s=215028346, n=28751225244, pv=d5 Ne5 Nxe5 Bxe5 f4 Bg7 Be3 Nf6 Be2 O-O O-O Bd7 Bf3 Ne8 Qd2 Qa5 a3 Rc8 Rfc1 b6 b4 Qa6 Bd4 e5 dxe6 Bxd4+ Qxd4 fxe6 Be2 Qb7 Rf1 b5 h3 a6 Kh2 Ng7 Rac1 Bc6 Bd3 Qe7 a4 bxa4 Bxa6 Bb7 Bxb7 Qxb7 b5 Qd7 Qxa4, tb=null, h=68.0, ph=0.0, wv=0.48, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Ne5 {d=33, sd=88, mt=53083, tl=1680354, s=146710264, n=7786647307, pv=Ne5 Nxe5 Bxe5 Be3 f5 Bd4 Nf6 exf5 Bxf5 Bb5+ Kf7 O-O Rc8 Bxe5 dxe5 Ba4 Qd6 Qe2 a6 Bb3 b5 h3 Rhd8 Rfe1 b4 Na4 Be4 Qe3 Rb8 Rad1 Bxd5 Bxd5+ Nxd5 Qg3 Rd7 Re3 Rb5 Rxe5 Kf6 Qf3+ Kg7 Rde1 Nc7 R5e4 e6 Qe3 Qd2 Qa7 Rf5 R4e2 Qd6 Qe3 Rd5 Rc2 Rd3 Qc1 Rd1 Rxd1 Qxd1+ Qxd1 Rxd1+ Kh2 Nd5 Nc5 Ra1 b3 Kf6 Nxa6 g5 Nc5 h5 Nd3 h4, tb=null, h=49.8, ph=0.0, wv=0.73, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
8. Nxe5 {d=41, sd=72, mt=51453, tl=1566486, s=195474887, n=10057182961, pv=Nxe5 Bxe5 f4 Bg7 Be3 Nf6 Be2 O-O O-O Bd7 a3 a5 Bf3 Rc8 h3 b5 Rc1 Rc4 Ne2 Qb8 Rxc4 bxc4 Qd2 Rc8 Rc1 a4 Bd4 h5 Kh2 Bh6 Qc3 Qb5 Kh1 Bg7 g4 hxg4 hxg4 Qb3 g5 Nh5 Bxg7, tb=null, h=28.1, ph=0.0, wv=0.36, R50=50, Rd=-9, Rr=-1000, mb=+0+1+0+0+0,}
Bxe5 {d=36, sd=100, mt=92103, tl=1591251, s=153102731, n=14098005753, pv=Bxe5 Be3 f5 Bd4 f4 Bb5+ Kf7 Qd2 Nf6 f3 Bxd4 Qxd4 Rf8 O-O-O a6 Be2 b5 Kb1 Nd7 g3 Kg8 Qd2 Nc5 b4 Na4 Nxa4 bxa4 gxf4 Rb8 Ka1 a3 f5 Qb6 Rb1 Qf2 Rhg1 Qxh2 f4 Bd7 Qe3 Bb5 Bxb5 axb5 fxg6 hxg6 Rxg6+ Kf7, tb=null, h=71.2, ph=0.0, wv=0.85, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
9. f4 {d=40, sd=70, mt=48965, tl=1520521, s=207911997, n=10179579296, pv=f4 Bg7 Be3 Nf6 Be2 O-O O-O Bd7 a3 Be8 Rc1 a6 Qd2 Nd7 Nd1 h6 Nf2 Kh7 h3 Nf6 Bf3 Nd7 Rfe1 a5 Bd4 Bxd4 Qxd4 Nc5 Rc3 a4 e5 Nb3 Qb4 b5 Ne4 Qb6+ Kh1, tb=null, h=28.9, ph=0.0, wv=0.41, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Bg7 {d=33, sd=84, mt=57511, tl=1536740, s=141064712, n=8111503113, pv=Bg7 Be3 Nf6 Be2 O-O O-O Bd7 a4 Rc8 Bd4 Qa5 h3 Rfd8 Kh2 Ne8 Bxg7 Kxg7 Bd3 a6 Qf3 Kg8 Rf2 Qb4 e5 Ng7 g4 Rf8 Qg3 b5 a5 Kh8 Rg1 f5 exf6 exf6 f5 gxf5 gxf5 Rf7 Qf4 Qxf4+ Rxf4 Nh5 Rh4 Ng7 Rf1 Re7 Rg4 Re3, tb=null, h=51.9, ph=0.0, wv=0.59, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
10. Be3 {d=38, sd=72, mt=47598, tl=1475923, s=187100638, n=8904680676, pv=Be3 Nf6 Be2 O-O O-O a5 Rc1 Bd7 Bf3 a4 a3 Rc8 Bd4 Qa5 Kh1 Bb5 Re1 Nd7 Bxg7 Kxg7 Bg4 Rc7 Qd4+ Kg8 Bxd7 Bxd7 e5 Qa6 h3 dxe5 fxe5 Rc4 Qe3 Rfc8 Rf1 Bf5 Rce1 h5 Qh6 Bd3 Rf3, tb=null, h=27.0, ph=0.0, wv=0.62, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Nf6 {d=33, sd=101, mt=55096, tl=1484644, s=146631119, n=8077321874, pv=Nf6 Be2 O-O O-O Bd7 a4 Qa5 Bd4 Ne8 h3 Rc8 Bb5 Qc7 Kh2 a6 Be2 Bxd4 Qxd4 Qc5 Rfd1 a5 e5 h5 Qxc5 Rxc5 Ra3 Nc7 exd6 exd6 Rb3 Rb8 Rb6 Ne8 Rd4 Kf8 g4 hxg4 hxg4 Rc7 Kg3 Rbc8 Ne4, tb=null, h=48.7, ph=0.0, wv=0.74, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
11. Be2 {d=38, sd=73, mt=41188, tl=1437735, s=185015118, n=7619292595, pv=Be2 O-O O-O Bd7 a3 Qb8 Bf3 Rc8 Bd4 Ne8 h3 a5 Bxg7 Nxg7 Qd2 b5 Ne2 Qb6+ Kh2 Rc4 b3 Rc7 Rfc1 Rac8 b4 Rxc1 Rxc1 Rxc1 Qxc1 Be8 bxa5 Qxa5 Nd4 h5 Qe3 b4 axb4 Qxb4 Nc6 Bxc6 dxc6, tb=null, h=24.1, ph=0.0, wv=0.36, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
O-O {d=32, sd=100, mt=57038, tl=1430606, s=152084212, n=8672754291, pv=O-O O-O Bd7 a4 Qa5 Bd4 Rfd8 Ra3 Rac8 h3 Ne8 Bxg7 Nxg7 Kh2 a6 Qd2 Qc5 Rb3 Rc7 Rd1 Rdc8 Bf1 Ne8 e5 h5 Be2 Rd8 Bf3 Qc4 Rb6 Qc5 Qd4 Bc8 Ne4 Qxd4 Rxd4 Rc1 Ng5 f6 exf6 Nxf6 Ne6 Re8 Kg3 Re1 Kh4 Kh7 Ng5+ Kg7 Rb3 Rc1 Ne6+ Kh6 Kg3 a5 Rb5, tb=null, h=48.3, ph=0.0, wv=0.75, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
12. O-O {d=36, sd=71, mt=36245, tl=1404490, s=189222145, n=6857410569, pv=O-O Bd7 a3 a5 Bf3 Qb8 Bd4 b5 e5 Ne8 Re1 b4 axb4 axb4 Rxa8 Qxa8 Ne4 Qb8 Kh1 Bc8 Ng3 Bh6 f5 dxe5 Rxe5 Qc7 Qe1 Qc4 Be3 Bxe3 Rxe3 Nd6 fxg6 hxg6 Rxe7 Qc5 Qd2 Nc4 Qe2 Ba6 d6 Qxd6, tb=null, h=20.4, ph=0.0, wv=0.23, R50=47, Rd=7, Rr=-1000, mb=+0+0+0+0+0,}
Bd7 {d=34, sd=93, mt=40928, tl=1392678, s=146709875, n=6002781255, pv=Bd7 a4 Rc8 Bd4 Qa5 h3 Rfd8 Ra3 Ne8 Bxg7 Nxg7 Kh2 Qc5 Qd2 a6 Rfa1 Rc7 Rb3 Qa5 Rc1 Ne8 Qd4 Bc8 Qe3 Ng7 Qd2 Rf8 Qd4 Rd8 Qe3 Qc5 Qd2 Qa5 Rf1 Bd7 Qe3 Bc8 g4 Ne8 Kg3 Qc5 Qxc5 Rxc5 h4 h6 Kf2 Nf6 g5 hxg5 hxg5 Nd7 Ke3, tb=null, h=38.7, ph=0.0, wv=0.79, R50=46, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
13. h3 {d=35, sd=72, mt=48870, tl=1358620, s=196918275, n=9621820796, pv=h3 a5 Rc1 a4 a3 Be8 Bd4 Qa5 Kh2 Nd7 Bxg7 Kxg7 Qd4+ f6 Rcd1 Qa7 Rf3 Qxd4 Rxd4 Nc5 Re3 Rb8 Rb4 Rf7 h4 h6 e5 fxe5 fxe5 dxe5 Rxe5 Kf8 Ne4 Nb3 Bc4 Rf4 d6 exd6 Nxd6 Rxh4+ Kg3 Rd4, tb=null, h=29.6, ph=0.0, wv=0.71, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
b5 {d=28, sd=83, mt=70709, tl=1324969, s=142387873, n=10066253080, pv=b5 Bf3 b4 Ne2 Bb5 Qd2 Qa5 Rfc1 Bxe2 Qxe2 Nd7 Qd2 Rfd8 Kh2 Rab8 Rc4 g5 g3 Rbc8 Rxb4 Rb8 Rxb8 Qxd2+ Bxd2 Rxb8 Rb1 gxf4 gxf4 Rxb2 Rxb2 Bxb2 Bd1 Bd4 Kg3 Nc5 Bc2 Kg7 Kf3 Kf6 Ba5 Kg7 Kg3 Bf6 Kg4 Bd4 Kf3 Kf8 Bb4 Ke8 Kg4 Kf8 Bd2 Kg7 Be1 Kg8 Kg3 Kg7 Kf3 Kf8 Bb4, tb=null, h=60.0, ph=0.0, wv=0.68, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
14. Bf3 {d=35, sd=64, mt=30394, tl=1331226, s=201713479, n=6129669216, pv=Bf3 a5 Ne2 a4 Nd4 e6 dxe6 fxe6 Rc1 Qe8 Ne2 Bc6 Ng3 Qd7 Qd3 Rad8 Rfd1 Bb7 b4 Bh6 Ne2 Ne8 Kh2 Ba8 a3 Rf7 Nd4 Bxf4+ Bxf4 Rxf4 Nxb5 Qg7 g3 Rf7 Nc3 Qf6 Bg2 Qe5 b5 Nf6 b6 Bb7 Qc4 d5, tb=null, h=18.3, ph=0.0, wv=0.62, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Ne8 {d=31, sd=85, mt=46768, tl=1281201, s=150616757, n=7042086516, pv=Ne8 Qd2 Qa5 Kh2 Rc8 a3 Bxc3 bxc3 Qxc3 Qf2 Qc2 Qh4 f6 Rfc1 Qb2 Rxc8 Bxc8 Qe1 a6 Qd1 Qc3 Bd4 Qa5 Qc1 Bd7 Qe3 Qc7 Bb6 Qb8 Rc1 Rf7 Ba7 Qd8 Bd4 Ng7 Bb6 Qb8 Bc7 Qb7 Ba5 Ne8 e5 dxe5 fxe5 fxe5 Qxe5 Qa7 Rc3 Nd6 Rc7 Nc4 Qh8+ Kxh8 Bc3+ Kg8 Rxa7, tb=null, h=38.8, ph=0.0, wv=0.78, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
15. Qd2 {d=35, sd=69, mt=43873, tl=1290353, s=218923225, n=9603067274, pv=Qd2 Qa5 a3 Rc8 Rfc1 f5 exf5 Bxf5 Kh2 Rc7 Ra2 Qa6 b4 Qb7 Ne2 Nf6 Rc6 e5 Ng3 Rcf7 Nxf5 gxf5 a4 bxa4 Rxa4 Qb8 Raa6 Rd7 fxe5 dxe5 d6 f4 Bf2 e4 Bxe4 Kh8 Bd3 Nd5 b5 f3 gxf3 Rxf3, tb=null, h=27.1, ph=0.0, wv=0.72, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Qa5 {d=29, sd=89, mt=52905, tl=1231296, s=150670491, n=7969414283, pv=Qa5 a3 Rc8 Rfc1 f5 exf5 Bxf5 Kh2 Nf6 g4 Bd7 b4 Qa6 Bd4 Rfe8 Ne2 Rxc1 Nxc1 Qb7 Bg2 Qc7 Ne2 Rc8 Rc1 Qb8 Rd1 Rc4 Qe3 Qe8, tb=null, h=48.2, ph=0.0, wv=1.06, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
16. Rfc1 {d=44, sd=92, mt=237716, tl=1055637, s=183653237, n=43655843670, pv=Rfc1 Rc8 a3 Rc7 Kh2 Rc8 Bd4 Bxd4 Qxd4 Qb6 Qxb6 axb6 Be2 Nc7 Rf1 h5 Rad1 Kg7 Rd4 Rfd8 Kg3 Rh8 e5 Rhd8 Re1 Ne8 Bd3 Rc5 Re3 Nc7 Be2 Ne8 Kf2 h4 e6 fxe6 b4 Rcc8 dxe6 Bc6 Bxb5 Bxb5 Nxb5 Rc2+ Kf3 Ra8 Rdd3 Nc7 Nxc7 Rxc7 Rd5 Rc1 Rb5 Ra6 Rg5 Ra7 Rg4, tb=null, h=85.8, ph=0.0, wv=0.96, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Rc8 {d=32, sd=106, mt=35001, tl=1199295, s=157186564, n=5499486331, pv=Rc8 a3 Rc7 Kh2 f5 exf5 Bxf5 Bd4 Bxd4 Qxd4 Rc4 Qe3 Bd7 Qxe7 Rf7 Qe3 Rcxf4 Ne2 R4f5 Nd4 Re5 Qf2 Qb6 Qd2 Qd8 Nc6 Bxc6 dxc6 d5 a4 Qd6 Kh1 b4 a5 h5 Rd1 Nc7 Ra4 Qe6 Rf1 Qxc6 Rxb4 Qd6 Rb7 Qa6 Rb8+ Kg7 Rd1 Rfe7 b3 Ne6 Bxd5 Qd6 Qb4 Qxb4 Rxb4 Rd7, tb=null, h=35.0, ph=0.0, wv=0.82, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
17. a3 {d=38, sd=72, mt=34400, tl=1024237, s=208894060, n=7184284539, pv=a3 Rc7 Kh2 Rc8 Bd4 Bxd4 Qxd4 Qa6 g4 e5 dxe6 fxe6 Qe3 Qb7 Bg2 Rf7 f5 b4 axb4 Qxb4 Rc2 Qb6 Qh6 Qd4 Re2 Qe5+ Kg1 Rc7 Rf2 Bc6 Raf1 Qg7 Qxg7+ Nxg7 Rd1 gxf5 exf5 d5 fxe6 Nxe6 Bxd5 Bxd5 Rxf7 Kxf7 Nxd5 Rc2, tb=null, h=20.9, ph=0.0, wv=0.96, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Rc7 {d=34, sd=101, mt=42893, tl=1159402, s=159689889, n=6847183068, pv=Rc7 Kh2 f5 exf5 Bxf5 Bd4 Bxd4 Qxd4 Rc4 Qe3 Bd7 Qxe7 Rf7 Qg5 Rcxf4 Re1 R7f5 Qh6 Qd8 Ne4 Rh4 Qe3 Rhf4 Bg4 Rf8 Bxd7 Qxd7 Ng5 R4f5 Ne6 R8f7 Rad1 Nf6 Qb3 a6 Re2 a5 Qc3 Rxd5 Rxd5 Nxd5 Qxa5 Nc7 Nd4 Qd8 Nf3 Qf6 Rc2 d5 Qe1 Kg7 Kh1 h6 b4 Re7 Qd2 Kh7 Rc1 Qd6 Qd4 Re4 Qa7 Rc4 Rd1 Re4 Qb8 Kg7 Qa7 Rc4 Nd4 Qe5 Nxb5 Qe2 Rg1, tb=null, h=39.3, ph=0.0, wv=1.02, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
18. Kh2 {d=38, sd=75, mt=55700, tl=971537, s=193066137, n=10752239310, pv=Kh2 Rc8 Bd4 Bxd4 Qxd4 Qa6 Re1 Ng7 Qf2 f6 Qe3 Qb7 Ne2 Qb6 Qxb6 axb6 Nc3 Nh5 g3 Rc5 Re3 Ra8 Na2 Rc2+ Re2 Rc4 Rd1 Ng7 Nb4 g5 Re3 gxf4 gxf4 f5 Rg1 Kf7 Rg5, tb=null, h=29.6, ph=0.0, wv=0.75, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
f5 {d=33, sd=106, mt=36472, tl=1125930, s=149080797, n=5435038630, pv=f5 exf5 Bxf5 Bd4 Bxd4 Qxd4 Nf6 Ne2 Rxc1 Rxc1 Bd7 Qe3 b4 Qxe7 bxa3 bxa3 Qxa3 Rc6 Rf7 Qxd6 Qxd6 Rxd6 Ne8 Ra6 Bb5 Re6 Bd7 Re3 a5 Rb3 Nd6 Rb6 Nc4 Rb8+ Rf8 Rb7 Rf7 Kg3 Nd2 Rb2 Nxf3 Kxf3 a4 Ke3 Re7+ Kd4 a3 Ra2 Re8 Nc3 Ra8 Ke5 Kf7 Ne4 Ke7 d6+ Kf7 Nc5 Bc6 d7, tb=null, h=30.2, ph=0.0, wv=1.15, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
19. exf5 {d=43, sd=78, mt=77727, tl=896810, s=202571395, n=15742228311, pv=exf5 Rxf5 b4 Qa6 Bd4 Bxd4 Qxd4 Rf8 Ne4 Qb6 Qd2 Rxc1 Rxc1 Nf6 Re1 Nxe4 Rxe4 Rf7 Re1 a6 Be4 Qd8 Rc1 Qb6 g4 Rf8 Re1 Rf7 Kg2 Qa7 Rc1 Qb6 Bf3 Qa7 Kg3 Qb6 Qc3 Rf8 Re1 Rf7 Bg2 Qd8 Rc1 Qb6 Be4 Rf8 Re1 Qd8 Qd2, tb=null, h=41.7, ph=0.0, wv=1.07, R50=50, Rd=-9, Rr=-1000, mb=+1+0+0+0+0,}
Bxf5 {d=40, sd=98, mt=31796, tl=1097134, s=173056404, n=5499732522, pv=Bxf5 Bd4 Bxd4 Qxd4 Rc4 Qe3 Bd7 Qxe7 Rf7 Qg5 Rcxf4 Re1 R7f5 Qh6 Qd8 Ne4 Rh4 Qe3 Rhf4 Bg4 Rf8 Bxd7 Qxd7 Ng5 R4f5 Ne6 R8f7 Rad1 Nf6 Qb3 a5 Qc3 Nxd5 Qxa5 Nc7 Qb6 Nxe6 Rxd6 Qc7 Rxe6 Qxb6 Re8+ Kg7 Rxb6 Rg5 Re2 Rd5 Re3 Kh6 b4 Rfd7 Rg3 Kg7 Rc6 Kf7 Rf3+ Kg7 Re6 Rc7 Rb6 Kh6 Ra6 Rd2 Ra8 Re7 Rg3 Red7 Rb8 R2d5 Re8 Rc7 Ree3 Rcd7 Re6 Ra7 Re2 Kg7 Ree3 Rad7 Re1 Kf7 Rf3+ Kg7 Re6, tb=null, h=29.7, ph=0.0, wv=1.02, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
20. Qf2 {d=39, sd=81, mt=22863, tl=876947, s=222572359, n=5084888129, pv=Qf2 Nf6 Qe2 Rfc8 Qxb5 Qxb5 Nxb5 Rxc1 Rxc1 Rxc1 Bxc1 a5 b4 axb4 axb4 Kf7 Be3 Bd3 Na7 Bc4 b5 Nxd5 Bf2 e6 b6 Ba6 Nc6 Nxf4 Nb4 Bc8 b7 Bxb7 Bxb7 d5 Nc6 Nd3 Bb6 Bc3 g3 Ne5 Kg2 Nc4 Bc5 Kf6 Bc8 h5 Ba6 Be5 Be7+ Kf7 Bxc4 dxc4 Nxe5+ Kxe7 Nxg6+ Kd6, tb=null, h=12.0, ph=0.0, wv=1.50, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0+0+0,}
Bxc3 {d=38, sd=120, mt=61347, tl=1038787, s=182013194, n=11162869234, pv=Bxc3 bxc3 Rxc3 Qe1 Rxc1 Qxa5 Rxa1 g4 Ra2+ Kg3 Bc2 Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Nf6 Bxb5 Rc8 a4 Be4 Qf1 Rc2 a5 Bxd5 a6 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Bb5 Bg2+ Kh2 Bd5 Qa5 Rg5 Ba4 Rg2+ Kh3 Rg5 Qxd5+ Nxd5 a8=Q Nf6 Qb8 Re5 Kg2 Rg5+ Kf2 Rf5+ Kg3 Kg7 Bc2 Re5 Bb3 Re3+ Kf4 Re5 Kf3 Rg5 Kf2 Rf5+ Kg2 Re5 Ba4 Rg5+ Kf1 Re5 Bb3, tb=null, h=60.2, ph=0.0, wv=0.86, R50=50, Rd=-9, Rr=-1000, mb=+0-1+0+0+0,}
21. bxc3 {d=34, sd=74, mt=21959, tl=857988, s=230708631, n=5063362340, pv=bxc3 Nf6 Bd4 Qa4 g4 Be4 Bxe4 Nxe4 Qe3 Nf6 f5 Rf7 Qh6 Qb3 fxg6 hxg6 Qxg6+ Rg7 Qf5 Qxd5 Qxd5+ Nxd5 Bxg7 Kxg7 a4 a6 axb5 axb5 Rab1 Rc5 Rb3 Kf7 Rf1+ Kg6 h4 Nf6 Rf4 Rc4 Rb4 Rxc3 Rxb5 e5 h5+ Kf7 Rb7+ Ke6 Ra4 d5, tb=null, h=12.1, ph=0.0, wv=2.20, R50=50, Rd=-9, Rr=-1000, mb=+0-1+1+0+0,}
Rxc3 {d=44, sd=114, mt=40239, tl=1001548, s=176220637, n=7087417804, pv=Rxc3 Qe1 Rxc1 Qxa5 Rxa1 g4 Ra2+ Kg3 Bc2 Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Rh2 gxf5 Rg8 Qe1 Bxf5+ Kxf5 Rxh6 Kf4 Kf8 Bd1 Rf6+ Ke4 Rg5 Qh4 h6 Kd4 Ke8 Qh3 Rf4+ Kc3 Rc4+ Kb3 Re5 Qxh6 Rd4 Qh8+ Kd7 Qh3+ Kd8 Bg4, tb=null, h=47.3, ph=0.0, wv=1.17, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1+0+0,}
22. Qd2 {d=40, sd=94, mt=89045, tl=771943, s=237992267, n=21189165571, pv=Qd2 Rxc1 Qxa5 Rxa1 g4 b4 Qxb4 Rb1 Qa5 Bc8 Qxa7 Rb2+ Kg3 Rb7 Qd4 Rb3 a4 Ba6 Kf2 Ng7 Qa7 Rb2+ Kg1 Rb1+ Kg2 Rb3 Bd1 Rb1 Qxa6 Rxd1 Qc4 Ra1 Bb6 Ra8 a5 Kf7 Qc3 Ra2+ Kg3 Ra4 Kh4 h6 Kg3 Ne8 Qh8 Ra3+ Kf2 Ra2+ Ke3 Ra3+ Ke2 Ra2+ Kd3 Ra3+ Kc4 Ra4+ Kb3 Rxf4 Qh7+ Ng7 Qxh6 e5 dxe6+ Nxe6 Qh7+ Ng7 Kc3 Ra4 Bd4 Rxd4 Kxd4 Rxa5 Qh8, tb=null, h=37.9, ph=0.0, wv=3.41, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1+0+0,}
Rxc1 {d=45, sd=112, mt=39732, tl=964816, s=201465974, n=7999206536, pv=Rxc1 Qxa5 Rxa1 g4 Ra2+ Kg3 Bc2 Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Rc8 Bxb5 Nf6 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa5 Rg5 a8=Q Bxa8 Qxa8 Rc5 Kg2 Rg5+ Kf1 Kg7 Qd8 Re5 Kf2 Ne4+ Kg2 Nf6 Kf3 Nd5 Be4 Nf6 Bc2 Kf7 Qb8 Kg7 Bd3 Rh5 Qa7 Re5 Kf2 Kf7 Kg3 Rc5 Kg2 Rg5+ Kf3 Rc5 Kg2, tb=null, h=40.7, ph=0.0, wv=1.17, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-1+0,}
23. Qxa5 {d=42, sd=91, mt=23543, tl=751400, s=256305996, n=6031136399, pv=Qxa5 Rxa1 g4 b4 Qxb4 Rb1 Qd4 Bc8 Qxa7 Rb2+ Kg3 Rb7 Qd4 Rb3 a4 Ba6 Qe4 Rf7 Kf2 Rb2+ Ke1 Ra2 Qd4 Nf6 g5 Nd7 Qe4 Ra3 Bg4 Bd3 Qb4 Ra1+ Kf2 Bf5 a5 Bxg4 hxg4 Rf8 Qc3 Ra2+ Kg3 Rb8 Kf3 Rf8 Bd4 Ra4 Kg3 Rb8 Qe3 Kf8 Bg7+ Kxg7 Qxe7+ Kh8 Qxd7 Rb3+ Kh4, tb=null, h=12.2, ph=0.0, wv=3.70, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-1+1,}
Rxa1 {d=42, sd=114, mt=45981, tl=921835, s=191936490, n=8821784998, pv=Rxa1 g4 Bc2 Qxa7 Kf7 f5 Ra2 Kg3 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 Rc8 Kh4 h6 Bxb5 Nf6 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rg5 Qb1 Rh5+ Bh3 Bc6 Qb3+ Bd5 Qb2 Be4 Qa2+ Bd5 Qe2 Re5 Qd1 Rg5 Qe2 Re5, tb=null, h=41.1, ph=0.0, wv=1.29, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
24. g4 {d=31, sd=82, mt=29042, tl=725358, s=267113097, n=7753491891, pv=g4 b4 Qxb4 Rb1 Qd4 Bc8 f5 gxf5 Bh6 Rf6 Qc3 Ba6 Qc6 Rb2+ Kg3 Bb5 Qc3 Rb1 g5 Rxh6 gxh6 Nf6 Qc2 Re1 Qxf5 Bd7 Qg5+ Kf8 Kh2 Re5 Qg7+ Ke8 Qh8+ Kf7 Bg4 Bf5 Bxf5 Rxf5 Qa8 Rf2+ Kg1 Rf3 a4 Rxh3 Qxa7 Rg3+ Kf1 Rf3+ Ke2 Rh3 a5 Rxh6 a6 Rh2+ Kf3, tb=null, h=14.1, ph=0.0, wv=3.94, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
Ra2+ {d=40, sd=112, mt=52106, tl=872729, s=200971653, n=10468412450, pv=Ra2+ Kg3 Bc2 Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Rc8 Bxb5 Nf6 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rh4+ Kg3 Rg4+ Kf2 h5 Qa5 Rf4+ Ke1 Re4+ Be2 Re5 Kf2 Rf5+ Kg3 Re5 Qxd5+ Nxd5 a8=Q Nf6 Bf3 Re3 Qa7 Re5 Qb8 Re3 Kg2 Kg7 Qb5 Re5 Qb8 Rg5+ Kh2 Rc5 Kg2 Rg5+, tb=null, h=55.7, ph=0.0, wv=1.29, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
25. Kg3 {d=37, sd=93, mt=22577, tl=705781, s=277456093, n=6260241847, pv=Kg3 Bb1 Qc3 Bc2 Bxa7 h5 Bd4 hxg4 hxg4 Kf7 Qe1 Rg8 Qe2 Ng7 Bxg7 Rxg7 Qxb5 Rh7 Qc4 Bb1 f5 Rxa3 Qb4 Rxf3+ Kxf3 Bd3 fxg6+ Bxg6 Qf4+ Kg8 Qc4 Be8 Qc8 Kf8 g5 Rg7 Kf4 Rf7+ Ke3 Rh7 Ke2 Rg7 Qf5+ Rf7 Qg4 Rh7 Ke3 Bg6 Qe6 Be8 Qc8 Rf7 Qh3 Rg7 Qf5+ Rf7 Qg4, tb=null, h=14.2, ph=0.0, wv=3.90, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
Bc2 {d=46, sd=111, mt=25269, tl=850460, s=200548186, n=5060632945, pv=Bc2 Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Nf6 Bxb5 Rc8 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rg5 Qd4 Rg4 Qb2 Rh4+ Bh3 Rh5 Qd4 Be4 Qb6 Bd5 Qb2 Ne4 Qd4 Nf6 Qf2 Be4 Qd2 Ba8 Qc3 Bd5 Qd2 Kg6 Qe2 Kf7 Qxh5+ Nxh5 Bg2 Bxg2 Kxg2 Nf4+ Kf3, tb=null, h=27.6, ph=0.0, wv=1.29, R50=48, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
26. Qxa7 {d=43, sd=103, mt=21404, tl=687377, s=324387720, n=6938328952, pv=Qxa7 Kf7 f5 gxf5 Kh4 Ng7 Bh6 fxg4 Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Bxh3 Be7+ Ke8 Qb5+ Kxe7 Qxc4 Bg4+ Kh6 Rg6+ Kxh7 Bf5 Qc5+ Kf6 Qf8+ Ke6 Kh8 Bd3 Qc5 Kd7 Qa7+ Kc6 Qe3 Rd6 Qe8+ Kc7 Qe7+ Kc6 Kg8 Bc4+ Kf8 Re6 Qb4 Kd5 Qa5+ Kd4 Qd2+ Kc5 Qc3 Kd5 Kf7 Kc5 Qb4+ Kd4 Qd2+ Kc5 Kg7 Rc6 Qe3+ Kb5 Qa7 Ra6 Qd7+ Kc5 Qc8+ Kd4 Qb8 Kd5 Qb7+ Ke5 Qb2+ Kd5 Kf8, tb=null, h=9.8, ph=0.0, wv=3.68, R50=50, Rd=-9, Rr=-1000, mb=+0-1+1-2+1,}
Kf7 {d=49, sd=110, mt=28067, tl=825393, s=207912801, n=5830706610, pv=Kf7 f5 gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Nf6 Bxb5 Rc8 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rg5 Qd4 Rg4 Qb2 Rh4+ Bh3 Rh5 Qd4 Re5 Qf2 Rh5 Qf4 Re5 Qd2 Rh5 Qd1 Rg5 Qe2 Re5 Qd3 Rh5 Qc3 Rh4 Qa5 Rh5 Qa4 Ne4 Qd4 Nf6 Qb2 Rh4 Qc2 Rh5 Qb1 Ne4 Qf1+ Nf6 Kg3 Rg5+ Kh2 Rh5, tb=null, h=34.1, ph=0.0, wv=1.29, R50=49, Rd=-9, Rr=-1000, mb=+0-1+1-2+1,}
27. f5 {d=36, sd=98, mt=23478, tl=666899, s=388849302, n=9123182341, pv=f5 gxf5 Kh4 Ng7 Bh6 fxg4 Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Bxh3 Be7+ Ke8 Qb5+ Kxe7 Qxc4 Bg4+ Kh6 Rg6+ Kxh7 Bf5 Qc5+ Kf6 Qd6+ Kf7 Qf4 Kf6 Kh8 Rg4 Qf1 Rh4+ Kg8 Rc4 Qa1+ Ke7 a4 Be6+ Kh7 Rc6 Qe5 Kd8 Kg6 Kc8 Kg5 Kb7 Kf4 Bc8 Qd5 Kc7 a5 Rf6+ Kg3 Rg6+ Kh4 Re6 Qb3 Rc6 Kg5 Rc5+ Kf4 Rc6 Qb5 Ba6 Qe5+ Kd7 Qd5+ Kc7 Ke3, tb=null, h=1.1, ph=0.0, wv=4.13, R50=50, Rd=-9, Rr=-1000, mb=+0-1+1-2+1,}
gxf5 {d=41, sd=114, mt=47023, tl=781370, s=235833244, n=11084869989, pv=gxf5 Kh4 Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 h6+ Kh4 Nf6 Bxb5 Rc8 a4 Be4 a5 Rc2 a6 Bxd5 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rg5 Qd4 Rg4 Qb2 Rh4+ Bh3 Rh5 Qd4 Ba8 Qc4+ Bd5 Qc3 Re5 Qd2 Rh5 Qd3 Rg5 Qe2 Re5 Qf2 Rh5 Qd2 Ba8 Qc3 Bd5 Qb2 Rg5 Qa1 Ng4+ Kg1 Nf6+ Kf1 Rg3 Bc8 Bc4+, tb=null, h=45.2, ph=0.0, wv=1.29, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
28. Kh4 {d=45, sd=92, mt=34462, tl=635437, s=358390628, n=12344765203, pv=Kh4 Ng7 Bh6 fxg4 Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Bxh3 Be7+ Ke8 Qb5+ Kxe7 Qxc4 Bg4+ Kh6 Rg6+ Kxh7 Bf5 Qc5+ Kf6 Qf8+ Ke6 Kh8 Bd3 Qc5 Kd7 Qa7+ Kc8 Qa4 Rd6 Kg7 Ba6 Kf7 Kb7 Ke7 Rc6 Kd7 Rc7+ Kd6 Rc4 Qa5 Rc6+ Kd5 Rc4 Ke5 Rc6 Ke4 Kb8 Ke3 Bb7 Qe5+ Ka7 Qd4+ Kb8 Kd3 Ra6 Qd8+ Ka7 Qe7 Rb6 Qc5 Bc6 Kc3 Kb7 Qe7+ Ka6 Qc7 Bb7, tb=null, h=9.8, ph=0.0, wv=3.62, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
Ng7 {d=48, sd=101, mt=36113, tl=748257, s=211736752, n=7639250303, pv=Ng7 Bg5 Re8 Bh6 fxg4 hxg4 Bg6 Qd4 Rh2+ Kg5 Nf5 Qf4 Rh4 Qc1 Rh3 Qf1 Nxh6 Qxh3 Ng8 Be2 Nf6 Kh4 h6 Bxb5 Rc8 a4 Be4 a5 Bxd5 a6 Rc2 Qf1 Kg7 Qe1 Ne4 Qg1 Ra2 Bd3 Nf6 a7 Rg2 Qe1 Rxg4+ Kh3 Kf7 Qa1 Bg2+ Kh2 Bd5 Bf1 Rg5 Qb1 Rh5+ Bh3 Rh4 Qb2 Rh5 Qd4 Re5 Qd1 Rg5 Qe2 Re5 Qd3 Rg5 Qd2 Rh5 Qb2 Rh4 Qc2 Rh5 Qf2 Ba8 Qb6 Bd5 Qb4 Rg5 Qh4 Rh5 Qxh5+ Nxh5 Bg2 Bxg2 Kxg2 Nf4+ Kf1, tb=null, h=49.6, ph=0.0, wv=1.29, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
29. Bh6 {d=40, sd=113, mt=387628, tl=250809, s=356419038, n=5200510194, pv=Bh6 fxg4 Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bb4 Bf7+ Kh6 Rg8 Be7+ Ke8 Kxh7 Rg6 a4 Rgc6 Bg5 Rc7 Qb5+ R4c6 Qb8+ Rc8 Qe5+ Re6 Qb5+ Rec6 Kg7 Be6 Bf4 Bc4 Qb7 Bd5 Bd6 R6c7+ Qxc7 Rxc7+ Bxc7 Kd7 Bb6 Kc6 a5 Bc4 h4 Be2 Kf6 Kb5 Kf7 Kc6 Kg7 Kd7 Kf6 Kc6 Kf5 Kb5 Kf4 Bh5 Bd8 Ka6 Kf5 Be2 Kg5 Kb5 h5 Bxh5 Kxh5 Ka6, tb=null, h=4.6, ph=0.0, wv=3.99, R50=48, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
fxg4 {d=45, sd=125, mt=23403, tl=727854, s=259139196, n=6058156127, pv=fxg4 Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bb4 Bf7+ Kh6 Rg8 Be7+ Ke8 Kxh7 Rg6 a4 Rgc6 Bg5 Rc7 Qb5+ R4c6 Qb8+ Rc8 Qe5+ Re6 Qg7 Rec6 Qh8+ Kd7 Qd4+ Ke8 h4 Bg6+ Kg7 R8c7+ Kg8 Bf7+ Kh8 Rc8 Qb4 R6c7 Qb5+ Rc6 Kg7 Be6 Bf4 Bc4 Qb7 Bd5 Qb4 Bc4 Qf8+ Kd7 Qf5+ Ke8 Qf8+, tb=null, h=20.0, ph=0.0, wv=1.11, R50=50, Rd=-9, Rr=-1000, mb=-2-1+1-2+1,}
30. Bxg4 {d=30, sd=92, mt=19853, tl=233956, s=404749384, n=8028204050, pv=Bxg4 Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bb4 Bf7+ Kh6 Rg8 Be7+ Ke8 Kxh7 Rg6 a4 Rgc6 Bg5 Rc7 Qb5+ R4c6 Qb8+ Rc8 Qe5+ Re6 Qb5+ Rec6 Kg7 Be6 Bf4 Bc4 Qb7 Bd5 Bd6 R6c7+ Qxc7 Rxc7+ Bxc7 Kd7 Bf4 Kc6 a5 Bf3 h4 Be2 Kf6 Kb7 Kg5 Kc6 Be5 Kb7 Bd6 Ka6 Bc7 Kb7 Bd8 Kc6 Kf5 Kb7 Kf4 Ka6, tb=null, h=0.1, ph=0.0, wv=3.81, R50=50, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
Nf5+ {d=35, sd=112, mt=21778, tl=709076, s=310995855, n=6765714831, pv=Nf5+ Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb8+ Ke7 Qb7+ Kd8 Bf4 Rag2 Qb6+ Ke8 Bxd6 R2g7 a4 Bc8 a5 Kd7 Qc7+ Ke6 Qc6 Rg6 Bb4+ Kf5 Qc2+ Ke6 Qb3+ Ke5 Bc3+ Kd6 Bd2 Re8 Qb6+ Kd7 Qd4+ Ke6 Qc4+ Ke7 Qc7+ Kf6 Bc3+ Ke6 Bd4 Reg8 Be3 Kd5 Qf7+ Kd6 Bf4+ Kc6 Bd2 Kc5 Qb3 Kd6 Qd3+ Ke7 Qe4+ Kd8 Bf4 R8g7 Qd5+ Ke8 Qe4+ Kd7 Qf5+ Kd8 Qf8+ Kd7 Qf5+, tb=null, h=3.9, ph=0.0, wv=0.79, R50=49, Rd=-9, Rr=-1000, mb=-1-1+1-2+1,}
31. Bxf5 {d=27, sd=85, mt=8185, tl=228771, s=395261463, n=3227705109, pv=Bxf5 Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Bg5 Bc4 Qb8+ Rd8 Qc7 Re6+ Kf5 Rd5+ Kxe6 Rc5+ Kd6 Rxc7 Kxc7 Bb3 Kb6 Kd7 Kb5 Be6 h4 Bg4 a4 Kc7 a5 Be2+ Kc5 Kb7 Bd8 Ka6 Kd4 Kb5 Ke4 Bh5 Bb6 Bg4, tb=null, h=0.2, ph=0.0, wv=3.72, R50=50, Rd=-9, Rr=-1000, mb=-1+0+1-2+1,}
Bxf5 {d=41, sd=129, mt=40899, tl=671177, s=279654154, n=11429465287, pv=Bxf5 Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb8+ Ke7 Qb7+ Kd8 Bf4 Rc2 Bxd6 Rc4+ Kh5 Re8 Bf4 h6 h4 Re7 Qb5 Rd7 Kxh6 Rdd4 Bg5+ Kc8 Qb6 Bd7 Kg7 Ba4 Be7 Rg4+ Kh7 Rxh4+ Bxh4 Rxh4+ Kg7 Bd7 Qa6+ Kb8 Kg8 Kc7 Qa7+ Kc8 Qb6 Rg4+ Kh8 Re4 Qa6+ Kc7 Qa7+ Kd6 Qb6+ Bc6 Kg7 Rg4+ Kf7 Rf4+ Kg6 Rc4 Kf6 Rf4+ Kg5 Rc4 Kf6, tb=null, h=30.4, ph=0.0, wv=0.79, R50=50, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
32. Qd4 {d=33, sd=62, mt=139565, tl=92206, s=380944282, n=487989626, pv=Qd4 Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 Bg5 Bb3 h4 Bd1 Ke6 Bb3+ Ke5 Bd1 Kf4 Kd7 Ke3 Kc6 Kd2 Bh5 a4 Kb6 Bd8+ Kc6 a5 Kd7 Bg5 Kc6, tb=null, h=0.0, ph=0.0, wv=3.72, R50=49, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
Rg8 {d=48, sd=110, mt=23773, tl=650404, s=265335774, n=6300397962, pv=Rg8 Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qb7 Kd8 Qb6+ Ke7 Qb4 Rf8+ Kg3 Bc6 Qh4+ Kd7 Qxh7+ Kc8 Qa7 Rg8+ Kf4 Rf8+ Kg5 Rg8+ Kf5 Bd7+ Kf6 Bc6 Ke6 Rg6+ Kf5 Rg8 a6 Re8 Kf6 Bd5 Qb6 Kd7 Kg5 Bc6 Kf4 Bh1 Kg3 Ba8 Qb5+ Bc6 Qf5+ Kc7 Qa5+ Kd7 a7 Rg8+ Kh4 d5 Qb6 Re8 Kg3 d4 Qxd4+ Kc7 Qb4 Ra8 Qc5, tb=null, h=20.5, ph=0.0, wv=0.89, R50=49, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
33. Qf4 {d=43, sd=101, mt=8993, tl=86213, s=367076353, n=3293776117, pv=Qf4 e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 Bg5 Bb3 h4 Bd1 Ke6 Kf8 Ke5 Kf7 Ke4 Ke8 Ke3 Kd7 Kd2 Bh5 a4 Kc6 Kd3 Bd1 a5 Kb5 Bd8 Bg4 Ke4 Bh5 Ke5 Bd1 Kf5 Bh5 Ke4 Be2 Kf4 Bh5, tb=null, h=0.7, ph=0.0, wv=3.64, R50=48, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
e6 {d=40, sd=114, mt=19899, tl=633505, s=350730166, n=6970060589, pv=e6 dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa4+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qa7 Ke8 a6 Bc6 Qxh7 Kd8 Qa7 Ra8 Qb6+ Kd7 Kf1 Rf8+ Ke1 Re8+ Kd2 Rc8 Qa7+ Rc7 Qd4 Ba8 Qa4+ Rc6 a7 Kc8 Qb5 Kd7 Qf5+ Kc7 Qf8, tb=null, h=4.4, ph=0.0, wv=0.82, R50=50, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
34. dxe6+ {d=35, sd=87, mt=5796, tl=83417, s=368498673, n=2128079842, pv=dxe6+ Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 Bg5 Bb3 h4 Bd1 Ke4 Kd7 Kd3 Kc6 Kd2 Bf3 a4 Bh5 a5 Kb5 Bd8 Bf3 Bc7 Bh5 Kd3 Bf3 Bb6 Bh5 Ke3 Bd1 Bc7 Bh5 Kd2, tb=null, h=0.3, ph=0.0, wv=3.49, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
Kxe6 {d=36, sd=126, mt=19393, tl=617112, s=352375505, n=6824456414, pv=Kxe6 Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb6+ Ke7 Qb7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qa7 Kd8 Qb6+ Ke7 Qb4 Bc6 Qh4+ Kd7 Qxh7+ Kd8 Qa7 Ra8 Qb6+ Kd7 a6 Re8 Qa7+ Kc8 Kf1 Re4 Qb6 Kd7 Qb8 Re5 Kg1 Rg5+ Kf2 Rf5+ Kg3 Re5 Qa7+ Kc8 Qb6 Kd7 Qb8 Re3+ Kf2 Re8 Qb6 Ba8 Kf1 Bc6 Qa7+ Kd8 Kg1 Re6 Qf7 Re7 Qf8+ Kd7 a7 Re8 Qf5+ Kd8 Qf7 Re5 Qf8+, tb=null, h=1.0, ph=0.0, wv=0.81, R50=50, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
35. Qe3+ {d=24, sd=79, mt=3881, tl=82536, s=387266389, n=1494460999, pv=Qe3+ Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb8+ Kd7 Qb7+ Kd8 Bf4 Rc2 Bxd6 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 Ke5 Bb3 Kd6 Kd8 Bg5+ Kc8 Kc5 Kb7 Kb5 Be6 h4 Bd7+ Kc4 Bg4 a4 Kc6 a5 Be2+ Kc3 Kb5 Kd2, tb=null, h=0.2, ph=0.0, wv=3.40, R50=49, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
Kd7 {d=34, sd=106, mt=20501, tl=599611, s=365098028, n=7475017044, pv=Kd7 Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qa7 Kd8 Qb6+ Ke7 Qb4 Bc6 Qh4+ Kd7 Qxh7+ Kd8 Qa7 Ra8 Qb6+ Kd7 a6 Re8 Qa7+ Kc8 Kg3 Rg8+ Kh4 Re8 Kg4 Re4+ Kh5 Re5+ Kg6 Re6+ Kf7 Re8 Kf6 Bd5 Qb6 Kd7 Kg6 Bc6 Kg5 Re5+ Kf6 Bd5 a7 Re8 Kg7 Rc8 Kg6 Bc6 Kg5 Rg8+ Kf4 Re8 Qa5 Rf8+ Kg5 d5 Qc5 Re8 Kf4 Kc7 Qa5+ Kd6 Qa3+ Kc7 Qa5+, tb=null, h=0.6, ph=0.0, wv=0.77, R50=49, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
36. Qb3 {d=21, sd=54, mt=50363, tl=35173, s=373437727, n=149375091, pv=Qb3 Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb6+ Ke7 Qb7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Be6 a4 Rh2+ Kg3 Rh3+ Kf2 Rh2+ Kg1 Rc2 a5 Rc1+ Kf2 Rc2+ Ke1 Rc1+ Kd2 Rc5 Qb7+ Kd8 a6 Bd5 Qxh7 Bc6 Qh8+ Kc7 a7 Ra5 Qb8+ Kd7, tb=null, h=0.0, ph=0.0, wv=3.44, R50=48, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
Be6 {d=36, sd=101, mt=18256, tl=584355, s=356059472, n=6489896002, pv=Be6 Qxb5+ Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb8+ Kd7 Qb7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qa7 Kd8 Qb6+ Ke7 Qb4 Bc6 Qh4+ Ke8 Qxh7 Kd8 Qa7 Ra8 Qb6+ Kd7 a6 Re8 a7 Rc8 Ke1 Ba8 Kd2 Bf3 Kd3 Bd5 Qb5+ Bc6 Qa5 Ba8 Qa6 Bf3 Qa1 Ba8 Qa5 Bb7 Kd4 Ba8 Qb6 Rc5 Qb8 Rc8 Ke3 Bc6 Kf4 Bd5 Kg3 Bc6 Kg4 Re8 Kf5 Rc8 Kf6 Bd5 Qb6 Rf8+ Kg5 Rg8+ Kf6 Rf8+, tb=null, h=1.4, ph=0.0, wv=0.76, R50=48, Rd=-9, Rr=-1000, mb=-1+0+0-2+1,}
37. Qxb5+ {d=17, sd=66, mt=2743, tl=35430, s=386094221, n=1049790189, pv=Qxb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Bd5 h4 Re6 Bg5 Rc6 h5 Kf7 a4 Be6+ Kf4 Rc2 Ke3 Rc3+ Kf2 Rd5 Qb7+ Kg8 Bh6 Rc2+ Ke1 Rg2 Qb4 Rf5 Qb8+ Kh7 Qb7+ Kxh6 Qxg2, tb=null, h=0.2, ph=0.0, wv=3.31, R50=50, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
Kc7 {d=37, sd=108, mt=17623, tl=569732, s=359877037, n=6331316720, pv=Kc7 Qa5+ Kd7 Qa7+ Kd8 Qb8+ Kd7 Qb7+ Kd8 Bg5+ Rxg5 Qa8+ Kc7 Kxg5 Bxh3 Qa5+ Kd7 Qb5+ Ke7 Kh4 Bd7 Qg5+ Kf7 Qh5+ Kf8 Qd5 Rh2+ Kg3 Rh6 Qa8+ Ke7 a4 Rh3+ Kf2 Rc3 a5 Rc8 Qa7 Kd8 Qb6+ Ke7 Qb4 Rf8+ Kg1 Rg8+ Kh2 Rc8 Qh4+ Ke8 a6 Bc6 Qxh7 Kd8 Qa7 Ra8 Qb6+ Kd7 Kg1 Rg8+ Kf1 Rg5 Qb8 Rf5+ Kg1 Rg5+ Kf2 Rf5+ Kg3 Re5 Qa7+ Kc8 Qb6 Kd7 Qb8 Re3+ Kf2 Re8 Qa7+ Kc8 Kf1 Re5 Qb6 Kd7 a7 Re8 Kg1 Rg8+ Kf2 Re8 Qb8 Rc8 Ke3 Ba8 Kf2 Bd5 Qb5+ Bc6 Qb6 Re8, tb=null, h=0.2, ph=0.0, wv=0.76, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
38. Qa5+ {d=34, sd=92, mt=9517, tl=28913, s=353590013, n=3356630001, pv=Qa5+ Kd7 Qb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Re1 h4 Be6+ Kg6 Bf7+ Kg5 Rd1 Qe5+ Re7 Qb8+ Kd7 Qb7+ Ke8 Qc8+ Rd8 Qc6+ Rdd7 Qb5 Bd5 h5 Re5+ Kh4 Re4+ Kg3 Be6 Kf3 Bd5 Kf2 Be6 Qb8+ Rd8 Qb2 Bc4 Qh8+ Kd7 Qh7+ Re7 Qf5+ Kc6, tb=null, h=1.6, ph=0.0, wv=3.89, R50=49, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
Kd7 {d=38, sd=94, mt=18175, tl=554557, s=359103529, n=6513060710, pv=Kd7 Qa4+ Ke7 Bg5+ Rxg5 Kxg5 Bxh3 Qe4+ Be6 Qxh7+ Bf7 Qe4+ Be6 a4 Rf2 Qa8 Rf5+ Kh4 Bd5 Qa7+ Kd8 a5 Bc6 Qb8+ Kd7 a6 Re5 Kg3 Re3+ Kh2 Re8 Qa7+ Kc8 Qb6 Kd7 Kg3 Re5 a7 Re8 Kg4 Rg8+ Kf4 Re8 Kf5 Rf8+ Kg6 Re8 Kf6 Rf8+ Kg5 Rc8 Qb8 Ba8 Kg4 Bc6 Kf4 Ba8 Kg5 Bc6 Qb6 Rg8+ Kf6 Rf8+, tb=null, h=0.7, ph=0.0, wv=0.75, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
39. Qb5+ {d=21, sd=75, mt=4157, tl=27756, s=406484259, n=1679186476, pv=Qb5+ Kc7 Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Re4 Bg5 Bf7+ Kf5 Bd5 h4 Rc4 Bf4 Ke7 h5 Be6+ Kg6 Bd5 Be5 Rg4+ Kh6 Ke6 Bb8 Re4 a4 Ke7 Qb2 Re6+ Kg5 Kf7 Qh8 Bc6 Bf4 Rd1 Qh7+ Ke8 a5 Rg1+ Kh4, tb=null, h=0.1, ph=0.0, wv=3.40, R50=48, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
Kc7 {d=86, sd=86, mt=18174, tl=539383, s=234985046, n=4263568678, pv=Kc7, tb=null, h=20.1, ph=0.0, wv=0.00, R50=47, Rd=7, Rr=-1000, mb=+0+0+0-2+1,}
40. Bf4 {d=19, sd=60, mt=16952, tl=13804, s=377956099, n=227529572, pv=Bf4 Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Bd5 h4 Re6 Bg5 Rc6 h5 Kf7 Qb2 Ra6 h6 Kg8 Bf6, tb=null, h=0.0, ph=0.0, wv=3.28, R50=47, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
Rc2 {d=36, sd=132, mt=21247, tl=521136, s=297751773, n=6317101617, pv=Rc2 Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bb4 Bf7+ Kh6 Rg8 Be7+ Ke8 Kxh7 Rg6 a4 Rgc6 Bg5 Rc7 Qb5+ R4c6 Qb8+ Rc8 Qe5+ Re6 Qg7 Rec6 Qh8+ Kd7 Qd4+ Rd6 Qg7 Kc6 Qxf7 Rd7 Be7 Rcc7 Qb3 Rxe7+ Kg6 Re5 Qb4 Re6+ Kg5 Kd7 h4 Rec6 Qb8 Rc5+ Kf6 Rh5 Qb4 Rhc5 Qe1 R5c6+ Kf5 Rc5+ Kg6 Kc8 Qe6+ Kb7 Qb3+ Kc8 Qh3+ Kb8 Qd3 R5c6+ Kg5 Rc5+ Kh6 R7c6+ Kg7 Rc7+ Kg8 Rc8+ Kf7 R8c7+ Kg6, tb=null, h=12.1, ph=0.0, wv=0.71, R50=46, Rd=-9, Rr=-1000, mb=+0+0+0-2+1,}
41. Bxd6+ {d=30, sd=79, mt=1596, tl=15208, s=319690023, n=502233027, pv=Bxd6+ Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Bd5 h4 Re6 Bg5 Rc6 h5 Kf7 a4 Be6+ Ke4 Rc4+ Ke3 Rc3+ Kf2 Rd5 Qb7+ Kg8 Bh6 Rc2+ Ke1 Rg2 Be3 Rg7 Qb6 Re7 Qb8+ Kh7 Qf8 Rg7 Qf6 Bh3 Bf4 Rf5 Qh6+ Kg8 Qe6+ Rff7 Qd6, tb=null, h=0.5, ph=0.0, wv=3.21, R50=50, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Kd8 {d=43, sd=108, mt=15704, tl=508432, s=273923537, n=4293203600, pv=Kd8 Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 h4 Kd7 Bf4 Bf3 Bg5 Bd1 Ke5 Kc8 Kd6 Kb7 Bf4 Kb6 Ke6 Kb5 Kf5 Kc6 Be3 Kd7 Ke5 Kc6 Kd4 Kd6 Bf4+ Kc6 Ke5 Kc5 Bg5 Kb5 Kf6 Kb6 Be3+ Kc6 Ke6 Bb3+ Ke7 Bd1 Bf4 Kb7 Be3 Kc6, tb=null, h=12.3, ph=0.0, wv=0.65, R50=49, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
42. Qb7 {d=28, sd=76, mt=1942, tl=16266, s=313210415, n=599797946, pv=Qb7 Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Bd5 h4 Re6 Bg5 Rc6 h5 Kf7 a4 Kf8 Bf6 Be6+ Kf4 Rc4+ Ke3 Kf7 Bb2 Re7 Qg5 Rg4 Qf6+ Ke8 a5 Bf7+ Kf3 Rb4 Qh8+ Kd7 Bf6 Bd5+ Kf2 Rf7 Kg3 Re4 Qd8+ Ke6 Qb6+ Kf5 Qc5, tb=null, h=0.7, ph=0.0, wv=3.89, R50=49, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Rc4+ {d=47, sd=114, mt=29605, tl=481827, s=235477411, n=6963538024, pv=Rc4+ Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 h4 Kd7 Bf4 Bf3 Bg5 Bd1 Ke5 Kc8 Kd6 Kb7 Ke7 Ka6 Ke6 Kb5 Kf5 Kc6 Be7 Bc2+ Kf6 Bd1 Ke5 Kb5 Kf5 Kc6 Bf6 Kd7 Ke4 Bb3 Bg5 Bd1 Kd5 Kc8 Bf4 Kb7 Kd4 Ka8 Be3 Bf3 Bd2 Bd1 Kd5 Kb7 Bf4 Ka8 Be3 Bf3+ Kd4 Bd1 Bf4 Bb3 h5 Bd1 h6, tb=null, h=38.4, ph=0.0, wv=0.65, R50=48, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
43. Kh5 {d=31, sd=82, mt=1984, tl=17282, s=314620517, n=616026974, pv=Kh5 Re8 Bf4 h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf6 Re4 Kf5 Bd5 h4 Re6 Bg5 Rc6 h5 Kf7 a4 Be6+ Kf4 Rc4+ Ke3 Rc3+ Kf2 Rd5 Qb7+ Kg8 Bh6 Rc2+ Ke1 Rg2 Be3 Rg7 Qb6 Bg4 Qb3 Rd7 a5 Be6 h6 Kh7 a6 Rf7 Qc3 Bg4 Qc2+ Bf5 Qa2 Be6 Qa4 Rh5 Qe4+ Bf5, tb=null, h=0.9, ph=0.0, wv=4.02, R50=48, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Re8 {d=50, sd=85, mt=17927, tl=466900, s=243894222, n=4364243212, pv=Re8 Bf4 h6 Kg6 Re7 Qb5 Bf7+ Kxh6 Rc6+ Qxc6 Re6+ Qxe6 Bxe6 h4 Bb3 Kg7 Bd1 Kf7 Kd7 Kf6 Bf3 Ke5 Kc6 Bg5 Bd1 Be3 Kc7 Bd4 Kc6 Bc3 Kb5 Be1 Kc6 Bd2 Kb5 Be1, tb=null, h=23.3, ph=0.0, wv=0.65, R50=47, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
44. Bf4 {d=31, sd=77, mt=2479, tl=17803, s=322726128, n=791324467, pv=Bf4 h6 Kg6 Rg8+ Kh7 Re8 Qb6+ Kd7 Qd6+ Kc8 Qb8+ Kd7 Qb5+ Ke7 Bd6+ Kxd6 Qxe8 Rc7+ Kxh6 Rc8 Qb5 Bxh3 Qd3+ Ke7 Kg5 Be6 Kf4 Rc6 Ke3 Bc8 Qh7+ Kd8 Kd4 Bd7 Qh8+ Kc7 Qe5+ Kb7 Qe7 Kc8 a4 Ra6 Qc5+ Kb8 a5 Rc6 Qe5+ Kb7 Qe7 Kc8 Qh7, tb=null, h=0.8, ph=0.0, wv=3.92, R50=47, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
h6 {d=50, sd=76, mt=22588, tl=447312, s=249680758, n=5631549506, pv=h6 Kg6 Re7 Qb6+ Ke8 Qb5+ Rd7 Bxh6 Bf7+ Kf5 Rc5+ Qxc5 Rd5+ Qxd5 Bxd5 h4 Kd7 Bf4 Bf3 Ke5 Kc6 Bg3 Kd7 Be1 Bd1 Kf5 Ke7 Bb4+ Kf7 Kg5 Kg7 Bd6 Kf7 h5 Kg7 h6+ Kg8 Bf4 Bc2 Bd2 Bb3 Kf6 Kh7 Ke5 Bd1 Be3 Bc2 Kd5 Bb3+ Kc5 Bc2 Kd4 Ba4 Bc1 Bd1 Bg5 Kg6 Bf4 Bc2 Kd5 Kh5 Kd4 Kg6, tb=null, h=24.4, ph=0.0, wv=0.75, R50=50, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
45. h4 {d=33, sd=77, mt=4242, tl=16561, s=320692476, n=1351077405, pv=h4 Re7 Qb8+ Rc8 Qb6+ Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf4 Rf5+ Kg3 Rfxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Kd8 Qb8+ Ke7 Qb4+ Kd8 a4 Kc7 Qf4 Rd6 a5 Be6 Qf2 Bc8 Qf7+ Bd7 Ke4 Ra6 Qh5 Bc8 Qe5+ Rd6 Ke3 Kc6 Qc3+ Kb7 Qg7+ Kb8 Qe5 Kc7 Qe7+ Kc6 Qa7, tb=null, h=1.7, ph=0.0, wv=4.10, R50=50, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Re7 {d=37, sd=85, mt=16620, tl=433692, s=269550301, n=4470491756, pv=Re7 Qb8+ Rc8 Qb6+ Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kh2 Rc2+ Kg1 Rff2 Qe5+ Kf7 Qd5+ Be6 Qb7+ Kg6 Qe4+ Bf5 Qe8+ Kg7 Qe5+ Kf7 Be3 Rg2+ Kh1 Bg4 Qd5+ Ke7 Bg5+ Ke8 Qd8+ Kf7 Qf6+ Kg8 Qg6+ Kf8 Bf4 Ke7 Qe4+ Be6 Qxg2 Rxg2 Kxg2 Bb3 Bg5+ Ke6 Kf3 Bd1+ Ke4 Kd6 Bf6 Bc2+ Ke3 Kd5 Kf4 Ke6 Be5 Bd1 Ke4 Ke7 Kd4 Kd7 Kc5 Ke8 Bf6 Kd7 Be5, tb=null, h=13.4, ph=0.0, wv=0.65, R50=49, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
46. Qb8+ {d=30, sd=77, mt=1703, tl=17858, s=307266532, n=514671442, pv=Qb8+ Rc8 Qb6+ Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf4 Rf5+ Kg3 Rfxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Ke7 Qb4+ Kd8 Qb7 Ke7 Qh1 Ra6 Qh4+ Ke8 Qc4 Rc6 Qg8+ Ke7 Qa8 Rc4 Qb7 Rg4 Qc7 Rg5+ Kd4 Rg6 Kc5, tb=null, h=0.8, ph=0.0, wv=4.15, R50=49, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Rc8 {d=42, sd=99, mt=13491, tl=423201, s=229013743, n=3081608929, pv=Rc8 Qb6+ Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kg2 Rc2+ Kg1 Rff2 Qb5+ Kf7 Qb3+ Be6 Qb7+ Kg6 Qe4+ Bf5 Qe8+ Kg7 Qe5+ Kf7 Be3 Rg2+ Kh1 Bg4 Qd5+ Ke7 Bg5+ Ke8 Qd8+ Kf7 Qf6+ Kg8 Qg6+ Kf8 Bf4 Ke7 Qe4+ Kd7 Qd5+ Ke7 Qxg2 Rxg2 Kxg2 Bd1 Kf2 Kd7 Ke1 Bg4 Be3 Be6 Bg5 Bf7 Kf2 Kc6 Kg3 Kd7 Kg4 Kc8 Bf4 Kd7 Kg5 Bb3 h5 Ke7 Kf5 Bc2+ Ke5 Bd1, tb=null, h=18.9, ph=0.0, wv=0.65, R50=48, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
47. Qb6+ {d=27, sd=73, mt=2039, tl=18819, s=304813426, n=612370174, pv=Qb6+ Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Bc8 Kd5 Rb7 Qe5+ Re7 Qh8+ Kd7 Qb2 Ba6 h5 Bc4+ Kd4 Ba6 Qb8 Rf7 Ke3 Re6+ Kf3 Ref6 Kg3 Rxf4 Qa7+ Kd8 Qxa6 R4f6 Qa5+ Ke8 Qc3 Kd7 Qd4+ Kc8 Qe5 Kb7 Qe4+ Ka7 Qe3+ Ka6 Qc3 Kb7 a4 Rb6, tb=null, h=1.6, ph=0.0, wv=4.32, R50=48, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
Ke8 {d=46, sd=89, mt=25830, tl=400371, s=226212683, n=5835382387, pv=Ke8 Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kg2 Rc2+ Kg1 Rff2 Qa8 Rfe2 Kf1 Kd7 Qd5+ Kc7 Bd8+ Kb8 Qb5+ Ka8 Qxe2 Rxe2 Kxe2 Bg4+ Ke3 Bd1 Bg5 Bh5 Kd4 Bg6 Ke5 Bh5 Bf4 Kb7 Kf6 Kc6 Kf5 Kd7 Kg5 Bd1 Be5 Ke7 Bf6+ Kf7 Bc3 Ke6 Bb4 Ba4 Bf8 Bb5 Bc5 Kf7 Bb4 Ba4 h5 Be8, tb=null, h=36.1, ph=0.0, wv=0.65, R50=47, Rd=-9, Rr=-1000, mb=+1+0+0-2+1,}
48. Kxh6 {d=28, sd=74, mt=1874, tl=19945, s=304441308, n=561389772, pv=Kxh6 Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Re7 Kf3 Bd7 Qh5+ Rf7 Kg3 Re6 Qh8+ Rf8 Qb2 Kf7 Qb7 Rg6+ Bg5 Rd6 Qb3+ Kg7 a4 Bc6 Qc3+ Kg8 Qc4+ Rf7 a5 Bb7 Qe2 Rg6 Kh2 Kh7 Qc2 Kg7 Qa2 Kg8 Qb3 Be4 Qa4 Bb7 Qc4 Kg7, tb=null, h=2.6, ph=0.0, wv=4.28, R50=50, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Bd5 {d=49, sd=85, mt=16693, tl=386678, s=226348437, n=3768927830, pv=Bd5 Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kh2 Rc2+ Kg1 Rff2 Qa8 Rfe2 Kf1 Kf7 Qf3+ Kg8 Bh6 Rf2+ Qxf2 Ba6+ Kg1 Rxf2 Kxf2 Bb5 Bg5 Kg7 Kg3 Kg6 Kf4 Kh5 Kf5 Bd7+ Ke5 Be8 Kd4 Kg4 Ke3 Bb5 Ke4 Be8 Kd5 Kf5 Kc5 Ke4 Kd6 Kf5 Kd5 Bf7+ Kd4 Bb3 Kc5 Ke6 Bf4 Kf5 Bd6 Kg4 Be7 Bf7 Bg5 Be8 Kb4 Kf5 a4 Ke6 Bd2 Kd5 a5 Kc6 Kc4 Kb7 Kd4 Ka6 Ke5 Bh5 Bc3 Bd1 Be1 Kb7 Bd2 Ka6 Kf5 Bc2+ Kg5 Bd1 Kf5, tb=null, h=23.1, ph=0.0, wv=0.75, R50=49, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
49. Qb5+ {d=28, sd=78, mt=2728, tl=20217, s=303338934, n=817801767, pv=Qb5+ Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf4 Rf5+ Kg3 Rgxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Kd8 Qb7 Ke7 Qh1 Re6+ Kd4 Ra6 Qh4+ Kd6 Qf6+ Be6 Qe5+ Kd7 Qb8 Rc6 a4 Rc8 Qa7+ Rc7 Qa5 Rc2 Qb5+ Rc6 a5 Bc4 Qb7+ Rc7 Qe4 Rc6, tb=null, h=2.0, ph=0.0, wv=4.45, R50=49, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rc6+ {d=49, sd=90, mt=15117, tl=374561, s=222845450, n=3360732234, pv=Rc6+ Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kh2 Rc2+ Kg1 Rff2 Qa8 Rfe2 Kf1 Kf7 Qf3+ Kg8 Qxe2 Rxe2 Kxe2 Bg4+ Kd2 Kf7 a4 Ke6 Bf4 Kd5 Kd3 Kc6 Kd4 Bd1 a5 Bg4 Bd2 Kb7 Bc3 Ka6 Kd5 Bf3+ Ke6 Bh5 Ke5 Kb7 Kd4 Ka6 Ke4 Kb7 Bd2 Ka6 Ke5 Bd1 Bc3 Bh5 Bb4 Bd1 Bd2 Bh5 Bc3, tb=null, h=22.7, ph=0.0, wv=0.75, R50=48, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
50. Kg5 {d=28, sd=77, mt=1805, tl=21412, s=303681900, n=538428010, pv=Kg5 Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Kf7 Qd8 Re6+ Be5 Re7 Qh8 Rg1 Qh5+ Kg8 Kd3 Re6 Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kb3 Rd6 Bc5 Rf6 a4 Kd8 Bb6+ Kc8 Bd4 Be8 Qe5 Re6 Qf5 Kb7 Ka3 Rh6, tb=null, h=2.7, ph=0.0, wv=4.22, R50=48, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rg7+ {d=50, sd=84, mt=23267, tl=354294, s=225384470, n=5236357406, pv=Rg7+ Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kh2 Rc2+ Kg1 Rff2 Qa8 Rfe2 Kf1 Kf7 Qf3+ Kg7 Bf6+ Kf7 Bd4+ Ke6 Qxe2+ Rxe2 Kxe2 Kd5 Be3 Bg4+ Kd3 Kc6 Ke4 Bd1 Bg5 Kb7 Bd2 Kc6 Kd4 Kb7 Bf4 Kc6 Ke5 Bf3 Bg5 Bd1 Kf4 Kd6 Ke4 Ke6 Be3 Bg4 Bd4 Be2 Be5 Bb5 Kd4 Be2 a4 Bd1 a5, tb=null, h=32.7, ph=0.0, wv=0.75, R50=47, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
51. Kf5 {d=29, sd=76, mt=1813, tl=22599, s=307028363, n=546817516, pv=Kf5 Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf3 Rf5+ Kg3 Rgxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Ke7 Qb4+ Kd8 Qb7 Rg5+ Kf6 Rf5+ Kg6 Rc5 Qb6+ Rc7 Qa5 Be6 Kf6 Bc4 Ke5 Kd7 Kd4 Be6 a4 Rc4+ Kd3 Rc6 Qa7+ Kc8 a5 Bc4+ Kc3 Ba6+ Kb3 Bc4+ Ka3 Ba6 Qa8+ Kc7 Qh8 Kd7 Qe5 Kc8 Qe8+ Kc7 Qf7+ Kc8 Kb4 Rc4+ Kb3 Rc6, tb=null, h=6.8, ph=0.0, wv=4.48, R50=47, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Be6+ {d=51, sd=88, mt=22198, tl=335096, s=227053353, n=5032183471, pv=Be6+ Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rc6 Qb8+ Bc8 Kf4 Rf7+ Kg3 Rc3+ Kh2 Rc2+ Kg1 Rff2 Qa8 Rfe2 Kf1 Rf2+ Ke1 Rg2 Kd1 Kf7 Qf3+ Ke6 Qe4+ Kd6 Qd3+ Ke5 Qxc2 Bg4+ Kc1 Rxc2+ Kxc2 Kd4 Bf6+ Kc5 Kd3 Kb5 Ke4 Bd1 Bg5 Ka4 Be7 Kb5 Ke3 Kc6 Kf4 Kb5 Bf8 Kc6 Kg5 Kd7 Bc5 Ke6 Bb4 Bc2 Kf4 Bd1 Bc3 Kf7 Bd4 Ba4 Kg5 Bc2 h5 Kf8 h6 Kg8 Bc5 Kh7 Be3 Bd1 Kf5 Bc2+ Kg5, tb=null, h=30.7, ph=0.0, wv=0.75, R50=46, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
52. Ke4 {d=34, sd=71, mt=1954, tl=23645, s=306706474, n=589796550, pv=Ke4 Bd7 Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf3 Rf5+ Kg3 Rgxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Kd8 Qb7 Ke7 Qh1 Re6+ Kd4 Rd6+ Kc5 Ra6 Qh4+ Ke8 Qh8+ Ke7 Qe5+ Kd8 Qb8+ Ke7 Qg3 Kd8 Qd3 Rc6+ Kb4 Kc7 a4 Rb6+ Ka5 Rc6 Qd4 Rg6 Qe5+ Kc8 Qc5+ Rc6 Qd4 Rg6 Qh8+ Kb7 Qb2+ Kc8 Qb3 Kc7 Qc4+ Rc6 Qf7 Kd8 Qg7, tb=null, h=2.7, ph=0.0, wv=4.47, R50=46, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Bd7 {d=43, sd=70, mt=10857, tl=327239, s=240425498, n=2601163470, pv=Bd7 Qb8+ Kf7 Qd8 Re6+ Be5 Re7 Qxe7+ Kxe7 Bxg7 Kf7 Bd4 Kg6 Be3 Bc6+ Ke5 Ba4 Kd4 Bd1 Kc4 Kf7 Bg5 Ke6 Kb4 Be2 Be3 Kd5 a4 Kc6 Bf4 Bg4 Bg5 Kc7 a5 Kb7 Kc5 Bf3 Bd2 Ka6 Kd4 Bh5 Bc3 Bd1 Ke5 Kb7 Kf4 Ka6 Bb4 Be2 Kg5 Bd1 h5 Bxh5 Kxh5 Kb7 Be1 Ka6 Kg5 Kb7 Kf4 Ka7 Kg4 Kb7 Kf4, tb=null, h=9.1, ph=0.0, wv=0.76, R50=45, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
53. Qb8+ {d=30, sd=71, mt=3018, tl=23627, s=305457452, n=912095954, pv=Qb8+ Rc8 Qb3 Rc5 Bg5 Rg6 Kf4 Rf5+ Kg3 Rfxg5+ hxg5 Rxg5+ Kf4 Rg6 Ke5 Kd8 Qb7 Ke7 Qh1 Re6+ Kd4 Rd6+ Kc5 Ra6 Qe4+ Kd8 Qd3 Rc6+ Kb4 Rb6+ Ka5 Re6 a4 Rh6 Qe4 Kc8 Kb4 Rb6+ Ka3 Bc6 Qf5+ Kc7 Qh7+ Kc8 Qa7 Rb1 Qd4 Kb7 Qg7+ Ka6 Qc7 Rb6, tb=null, h=4.5, ph=0.0, wv=4.59, R50=45, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Kf7 {d=43, sd=80, mt=12226, tl=318013, s=230992873, n=2813724193, pv=Kf7 Qd8 Re6+ Be5 Re7 Qxe7+ Kxe7 Bxg7 Kf7 Bd4 Kg6 Be3 Bf5+ Kd5 Bd7 Bd2 Kh5 Be1 Kg4 Kc4 Kf5 Kb4 Be8 a4 Ke6 Bc3 Kd6 a5 Kc6 Kc4 Bf7+ Kd4 Kb7 Ke5 Ka6 Bd2 Be8 Bb4 Bf7 Bc3 Be8 Kd5 Bh5 Bd2 Bf7+ Ke5, tb=null, h=12.1, ph=0.0, wv=0.76, R50=44, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
54. Qd8 {d=28, sd=75, mt=1984, tl=24643, s=307404600, n=599746376, pv=Qd8 Re6+ Be5 Re7 Qh8 Rg1 Qh5+ Kg8 Kd3 Re6 Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Re6 Kb4 Kd8 a4 Rg8 Qd5 Rg4 Kc3 Kc8 h5 Rh4 Be5 Rxa4 h6 Rxh6 Qg8+ Kb7 Qb8+ Ka6 Qa8+ Kb6 Qd8+ Kb5 Qxd7+ Rc6+ Kd3 Ra3+ Ke4 Ra4+ Kf5 Rc4 Qd5+ Ka6 Qd3 Kb7 Bd4 Ka6 Kg5 Kb7 Be3 Kc7 Qh7+ Kc8, tb=null, h=16.1, ph=0.0, wv=4.85, R50=44, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Re6+ {d=47, sd=75, mt=13289, tl=307724, s=230668675, n=3056359953, pv=Re6+ Be5 Re7 Qxe7+ Kxe7 Bxg7 Kf7 Bd4 Kg6 Be3 Bc6+ Ke5 Bd7 Kd4 Be8 Kc5 Kf6 Kb4 Ke5 a4 Kd6 a5 Kc7 Kc3 Kb7 Kd4 Ka6 Bd2 Bh5 Bc3 Bg6 Ke3 Bh5 Bb4 Bd1 Kf4 Be2 Kg5 Bd1 Bc3 Be2 Bd2 Bd1 Kf4 Bh5 Be1 Bd1 Ke5 Bg4 Kf6 Bd1 Bd2 Kb5 Ke5 Ka6 Ke4 Bh5 Bb4 Be8 Kd4 Bf7 Ke5 Bh5 Kf6 Bg4 Be1, tb=null, h=18.6, ph=0.0, wv=0.76, R50=43, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
55. Be5 {d=27, sd=79, mt=2110, tl=25533, s=305114932, n=633418599, pv=Be5 Re7 Qh8 Rg1 Qh5+ Kg8 Kd3 Re6 Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Bd7 Kb4 Ke7 a4 Rg4 Qxg4 Rb6+ Ka5 Bxg4 Kxb6 Ke6 a5 Be2 Kc5 Kf5 Be3 Ke6 Bc1 Ke5 Kb6 Kf5 Bg5 Ke5 Kc6 Bf3+ Kb5 Kf5, tb=null, h=7.3, ph=0.0, wv=4.87, R50=43, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Re7 {d=50, sd=77, mt=15693, tl=295031, s=234441493, n=3669712700, pv=Re7 Qxe7+ Kxe7 Bxg7 Kf7 Bd4 Kg6 Be3 Bb5 Kd4 Be8 Kc5 Kf5 Bd2 Ke5 Bc3+ Ke4 Kb4 Kd5 a4 Bg6 a5 Kc6 Be1 Bf7 Kc3 Kb5 Kd4 Ka6 Ke4 Bg6+ Ke5 Be8 Kf5 Bh5 Kf4 Bd1 Ke5 Bg4 Bc3 Be2 Kd6 Bd1 Bd2 Bg4 Kd5 Bh5 Ke4 Bf7 Kf4 Bh5 Bc3 Bd1 Ke4 Be2 Kd4 Bg4 Be1 Be2 Bb4 Bd1 Bd2 Bf3 Ke5 Bh5 Ke6 Bg4+ Kd5, tb=null, h=21.7, ph=0.0, wv=0.76, R50=42, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
56. Qh8 {d=38, sd=87, mt=12530, tl=16003, s=297717538, n=3720278364, pv=Qh8 Rg1 Qh5+ Kf8 Kd3 Re6 Qf3+ Ke8 Bd4 Bc6 Qh5+ Rgg6 Kc3 Kd8 a4 Bd7 Kb4 Ra6 Qh8+ Be8 a5 Rad6 Bb6+ Kd7 Qe5 Rg4+ Ka3 Rd3+ Kb2 Rg2+ Kc1 Rdd2 Qb5+ Ke7 Bc5+ Kf7 Qb3+ Kg7 Bd4+ Kf8 Qb4+ Kg8 Qxd2 Rxd2 Kxd2 Kh7 Be3 Kg6 Kc3 Kf5 Kc4 Kg4 Bg5 Bc6 Kc5 Be4 a6 Bg2 Kb6 Kh3 Kc7 Bf1 Kb7 Bg2+ Kb6 Kg4, tb=null, h=16.8, ph=0.0, wv=5.53, R50=42, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rg1 {d=40, sd=91, mt=14048, tl=283983, s=230134470, n=3223493527, pv=Rg1 Qh5+ Kg8 Kd3 Re6 Bd4 Rd6 Ke3 Rg3+ Kf2 Rg4 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rdg6 Qe5+ Re6 Qh8+ Kf7 Qh5+ Rgg6 Qh7+ Ke8 Qh8+ Kf7 h5 Rg5 Qh7+ Ke8 h6 Bc6 Bg7 Kf7 Be5+ Ke8 Bf4 Rg2+ Kf1 Rf6 Qc7 Rh2 Qe5+ Kf7 Qxf6+ Kxf6 Bxh2 Bb5+ Kf2 Be8 Ke3 Bd7 Bf4 Kg6 Ke2 Bb5+ Kd2 Be8 Be3 Ba4 Kc3 Bb5 Bd2 Kh7 Be3 Kh8 Bf4 Kh7 Kd4 Be8 Bd2 Bc6 Bf4 Kg6 Be3 Bb5 Kc5 Be8 Kb4 Kh7 a4 Bd7 a5 Bc6 Kc5 Be4 Kb6 Bf3, tb=null, h=18.5, ph=0.0, wv=0.76, R50=41, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
57. Qh5+ {d=26, sd=67, mt=1718, tl=17285, s=309444560, n=520795196, pv=Qh5+ Kg8 Kd3 Re6 Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kc3 Kd7 Kb4 Rh6 Qg4 Kc8 h5 Bd7 Qf3 Be8 Be3 Rxh5 Qa8+ Kd7 Qb7+ Kd8 Bb6+ Rxb6+ Qxb6+ Kc8 Qa6+ Kc7 Qa7+ Kd8 Qb8+ Ke7 Qg3 Rb5+ Kc4 Rf5 Qc7+ Kf6 Qd6+ Kf7 Kb4 Rb5+ Kc3 Rf5 Qb8 Rc5+ Kb4 Rd5 Qf4+ Kg8, tb=null, h=9.4, ph=0.0, wv=5.64, R50=41, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Kg8 {d=36, sd=105, mt=18494, tl=268489, s=238848565, n=4407711430, pv=Kg8 Kd3 Re6 Bd4 Rd6 Ke3 Rg4 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Kd8 Qa5+ Ke8 Qa8+ Kf7 Qf3+ Ke8 Bc5 Re6+ Kf2 Rg8 Qa8+ Kf7 Qb7 Ke8 Qd5 Bc8 Qa8 Kd7 h5 Rf6+ Ke1 Re8+ Kd2 Ba6 Qd5+ Kc7 Qg5 Re2+ Kc3 Rf3+ Kb4 Re4+ Ka5 Bb7 h6 Rf7 Qg6, tb=null, h=13.7, ph=0.0, wv=0.96, R50=40, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
58. Kd3 {d=31, sd=82, mt=5385, tl=14900, s=304817402, n=1630468286, pv=Kd3 Re6 Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kb3 Kd7 a4 Rh6 Qg4 Rg6 Qf5 Rg3+ Kb4 Rf3 Qg4 Rd3 Bb6 Bd5 Qg7+ Re7 Qg5 Rb3+ Ka5 Be4 Qg4+ Kd6 Qf4+ Kd5 Qf6 Re5 Bc5 Re6 Qf7 Rd3 h5 Ke5 Qc7+ Kf5 Qg7 Rd8 h6 Rg6 Qf7+ Kg4 Qc4, tb=null, h=8.4, ph=0.0, wv=5.89, R50=40, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Re6 {d=37, sd=85, mt=13765, tl=257724, s=239897924, n=3291879317, pv=Re6 Bd4 Rd6 Ke3 Bc6 Be5 Re6 Kd2 Rgg6 Kc3 Be8 Qh8+ Kf7 h5 Rg5 Bd4 Rc6+ Kd2 Rd5 h6 Rcd6 Qg7+ Ke6 Kc3 Rxd4 h7 Rh4 h8=Q Rc6+ Kb2 Rb6+ Ka1 Rxh8 Qxh8 Kd7 Qc3 Rc6 Qe5 Bf7 Qg7 Ke8 a4, tb=null, h=9.7, ph=0.0, wv=0.96, R50=39, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
59. Bd4 {d=29, sd=75, mt=1924, tl=15976, s=312429910, n=590180100, pv=Bd4 Rd6 Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kb3 Rd6 Bc5 Re6 a4 Kd7 Bd4 Rg3+ Kb4 Rf3 Qg4 Rd3 Bb6 Bf3 Qf5 Be4 Qf7+ Re7 Qf4 Rf3 Qb8 Ke6 Qg8+ Ref7 h5 Rf4 Kc3 Bd5 a5 Ke5 Qd8 Rf3+ Kd2 Rb3, tb=null, h=1.3, ph=0.0, wv=5.85, R50=39, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rd6 {d=32, sd=86, mt=13205, tl=247519, s=243877980, n=3210409734, pv=Rd6 Ke3 Rgg6 Qh8+ Kf7 h5 Rde6+ Kf3 Bc6+ Kf4 Rg2 Qh7+ Ke8 Bc5 Kd8 h6 Be4 Qh8+ Kd7 Be3 Rh2 Kg3 Rh5 Kg4 Rd5 h7 Re7 Qa8 Rg7+ Kf4, tb=null, h=7.1, ph=0.0, wv=1.07, R50=38, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
60. Qh8+ {d=29, sd=74, mt=1548, tl=17428, s=315444156, n=476636120, pv=Qh8+ Kf7 Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Bd7 Kb4 Kd8 a4 Ra6 Qh8+ Be8 a5 Rad6 Bb6+ Kd7 Qe5 Rg4+ Ka3 Rd3+ Kb2 Rg2+ Kc1 Rdd2 Qb5+ Ke7 Bc5+ Kf7 Qb3+ Kg7 Bd4+ Kf8 a6 Rde2 Qb4+ Kf7 a7 Bc6 Qc4+ Ke7 Kd1 Rd2+ Ke1 Rge2+ Qxe2+ Rxe2+ Kxe2 Ke6 h5 Kf5 Be3 Kg4 h6, tb=null, h=7.2, ph=0.0, wv=6.03, R50=38, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Kf7
61. Qh7+ {d=37, sd=72, mt=5042, tl=15386, s=316322003, n=1583191628, pv=Qh7+ Ke8 Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Kd8 Qa5+ Kc8 Qa8+ Kc7 Qf3 Bc6 Qf4+ Kc8 h5 Rh6 Bb6 Rh7 Qg4 Re7 h6 Bf3 Qc4+ Rc6 Bc5 Re2+ Kb3 Re3+ Bxe3 Rxc4 Kxc4 Be4 Kb5 Kb7 Bg5 Bg6 Be7 Bf5 Kc5 Bc2 Kb4 Bf5 a4 Be4 Ka5 Kc6 Bh4 Bd3 Kb4 Bf5 Bd8 Kb7 Kc5 Bc2 Bg5 Ka6 Kb4 Bf5, tb=null, h=5.6, ph=0.0, wv=5.48, R50=37, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Ke8 {d=33, sd=75, mt=8853, tl=244629, s=227281982, n=2000990577, pv=Ke8 Qh5+ Rgg6 Kc2 Rc6+ Kb1 Re6 Ka2 Rc6 Kb2 Re6 Qh8+ Kf7 Kb3 Rg3+ Kb4 Rg4 Qh5+ Ke7 Qxg4 Rb6+ Bxb6 Bxg4 a4 Kd7 Be3 Kc6 a5 Be2 Bg5 Kb7 Bd2 Ka7 Bc3 Ka6 Kc5 Bf3 Kd4 Bd1 Ke4 Be2 Bb4 Bd1 Kd4 Kb5 Ke5 Ka6 Bc3 Be2 Kf5 Bd3+ Kg5 Be2 h5 Bxh5 Kxh5 Kb7 Bd2 Ka6 Bc3, tb=null, h=10.9, ph=0.0, wv=0.76, R50=36, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
62. Qh5+ {d=30, sd=77, mt=4285, tl=14101, s=306075878, n=1295007043, pv=Qh5+ Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Bd7 Kb4 Kd8 a4 Ra6 Qh8+ Be8 a5 Rad6 Bb6+ Kd7 Qe5 Rg4+ Ka3 Rd3+ Kb2 Rg2+ Kc1 Rdd2 Qb5+ Ke7 Bc5+ Kf7 Qb3+ Kg7 Bd4+ Kf8 a6 Rde2 Qb4+ Kf7 a7 Rc2+ Kd1 Bc6 Qb3+ Ke7 Qxc2 Rxc2 Kxc2 Ke6 h5 Kf5 Be3 Kg4, tb=null, h=2.4, ph=0.0, wv=6.16, R50=36, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rgg6 {d=34, sd=83, mt=16423, tl=231206, s=235872334, n=3863352970, pv=Rgg6 Kc3 Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kb3 Rd6 Kc3 Re6 Qf5 Bd7 Qf3 Bc6 Qf4 Bd5 Kb4 Re4 Qb8+ Kf7 Kc5 Bc6 Qc7+ Re7 Qf4+ Ke8 h5 Rge6 h6 Kd7 Qg4 Be4 Kb4 Bh7 Bc5 Rf7 a4 Rf4+ Qxf4 Re4+ Qxe4 Bxe4 Be3 Kc8 Kc5 Bb1 Kd4 Kb8 Bf4+ Ka8 a5 Kb7 Be3 Ka6 Bd2 Bh7 Ke5 Bc2 Kd5 Ka7 Kd4 Ka6 Ke5 Bg6 Bc3 Bb1 Kf6 Be4 Kg7 Bc2 Kf6 Be4, tb=null, h=13.0, ph=0.0, wv=0.96, R50=35, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
63. Kc3 {d=28, sd=74, mt=2164, tl=14937, s=313823134, n=666874161, pv=Kc3 Bh3 Kb2 Bc8 Qb5+ Bd7 Qe5+ Rde6 Qb8+ Kf7 Qf4+ Ke8 Bc5 Kd8 Qb8+ Bc8 h5 Re2+ Kb3 Rc6 a4 Re1 Kb2 Ree6 Qf4 Ba6 Be3 Bd3 h6 Rc2+ Kb3 Ke8 Qg5, tb=null, h=1.1, ph=0.0, wv=6.31, R50=35, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Rc6+ {d=29, sd=92, mt=15714, tl=218492, s=239987835, n=3761329349, pv=Rc6+ Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Bd7 Qf3 Ra6 Kb2 Ra5 Bc5 Kd8 h5 Rb5+ Ka1 Re6 Bb4 Be8 Qg4 Reb6 Qh4+ Kc8 Qc4+ Kd7 Qd4+ Kc8 Qh8 Kd7 Qg7+ Kc8 h6 Rh5 h7 Rbh6 Qf8 Rh1+ Kb2 R1h2+ Kb3 R2h3+ Kc4 Rc6+ Bc5 Rh4+ Kb3 Rh3+ Kb4 Rh4+ Kc3 Rh3+ Kd4 Rh4+ Kd5 Rh5+ Kd4 Rh4+, tb=null, h=8.3, ph=0.0, wv=1.11, R50=34, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
64. Kb2 {d=28, sd=64, mt=1790, tl=16147, s=309663609, n=542530643, pv=Kb2 Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Bc6 Kb3 Rd6 Bc5 Re6 a4 Kd7 Bd4 Rg3+ Kb4 Rf3 Qg4 Rd3 Bc5 Bd5 Ka5 Kc6 Bb6 Bb3 h5 Rdd6 h6 Rxh6 Qc8+ Kd5 Bc7 Bc4 Qf5+ Kd4 Qf2+ Ke4 Bxd6 Rxd6 Kb4 Ba6 Qg2+ Kf5 Qf3+ Ke6 Qh3+ Ke7 Qh7+ Kf8 Kc5 Re6 Qd7 Rg6 a5 Rf6 Qh7 Be2 Qc7, tb=null, h=6.7, ph=0.0, wv=6.20, R50=34, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
Re6 {d=36, sd=88, mt=12000, tl=209492, s=242013811, n=2893033104, pv=Re6 Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Bd7 Qf3 Ra6 Kb2 Ra5 Bc5 Kd8 h5 Rb5+ Ka1 Re6 Bb4 Ra6 Qf1 Be8 Qf8 Re6 h6 Rh5 Bd2 Rh1+ Kb2 Rh2 Kc3 Rc6+ Kd3 Kd7 Qg7+ Kc8 Qg8 Kd7 Bf4 Bg6+ Kd4 Rhc2 a4 R2c4+ Qxc4 Rxc4+ Kxc4 Kc6 Be3 Kb7 Kd4 Kb8 Bf4+ Kb7 Bg5 Ka6 Bd2 Bh7 Bc3 Bb1 a5 Bc2 Ke5 Kb7 Bb4 Ka6 Kf6 Bb1 Bc3 Kb7 Bd4 Ka6 Bc3, tb=null, h=10.4, ph=0.0, wv=1.22, R50=33, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
65. Qh8+ {d=26, sd=71, mt=2359, tl=16788, s=309736842, n=718279737, pv=Qh8+ Kf7 Qh7+ Ke8 Qh5 Ba4 Kc3 Ra6 Be3 Re6 Qh8+ Kf7 Bc5 Rg3+ Kb4 Rg4+ Ka5 Bc6 Qf8+ Kg6 h5+ Kg5 Be7+ Rxe7 Qxe7+ Kxh5 Qh7+ Kg5 Qg7+ Kh5 Qh8+ Kg5 Qe5+ Kh6 Qf6+ Rg6 Qh4+ Kg7 a4 Bg2 Kb4 Bf1 Qe7+ Kg8 Qd7 Ba6 Kc5 Kf8 Kd4 Kg8 a5 Kf8 Qh7 Re6 Kc5 Rf6 Kd5, tb=null, h=1.3, ph=0.0, wv=6.41, R50=33, Rd=-9, Rr=-1000, mb=+2+0+0-2+1,}
*

"#;

        let pgn_info = get_pgn_info(sample_pgn).unwrap();
        assert!(pgn_info.out_of_book())
    }
}
