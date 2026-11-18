# How card recognition works

This document describes, end to end, how BrisMaster reads a game of briscola from the screen:
from the screenshot to the card entered in the game. It is meant for anyone who wants to change
the code. It describes how the program behaves as of version 0.6.9; for exact details the
reference is always the code. The comments in the code are in Italian, as are the names of the
tests and of the environment variables.

The recognition code lives in `crates/app/src/vision/`. The part that turns readings into a game
(confirmations, rule checks, recovery) is in `crates/app/src/assistant.rs`.

---

## 0. Ground rules (do not break them)

- **The app only reads pixels.** It does not touch the site, does not click, does not play on the
  user's behalf.
- **Learned data stays on the user's computer.** The card images already seen
  (`Documenti\BrisMaster\carte_viste.json`) and the site's table layout
  (`Documenti\BrisMaster\tavolo.json`) are never sent anywhere. The app's only network traffic is
  the update check on GitHub.
- **The neural network is trained only on the app's own reference decks** (section 6). Decks
  downloaded from elsewhere for testing (unknown licences) are used only for measuring, locally,
  and must never be committed or bundled in the app.
- **No regressions on the known decks.** Every rule written for decks the app has never seen must
  be measured against the user's recorded games and the extreme games (section 11) before it is
  published. In 0.6.9 several regressions came precisely from rules for unknown decks applied to
  known decks (section 12).

---

## 1. The pipeline at a glance

```
screen capture ──► Frames queue ──► analysis (detect.rs) ──► recognition (mod.rs::recognize)
 (capture.rs)        (mod.rs)        finds the cards and         scores of the 40 cards
                                     divides them into roles     (references + net + pips + memory)
                                              │
                                              ▼
                    assistant.rs: sightings ──► card certain? ──► entered in the game
                                                   │ no
                                                   └──► asks the user (confirmation)
                    + game rules as a check, recovery from history, restart from the screen
```

1. **Capture** (`capture.rs`, `screen.rs`): a screenshot of the monitor (or of the area chosen by
   the user), every 16 ms.
2. **Screenshot queue** (`mod.rs`, `Frames`): only screenshots that changed in the card area are
   kept; if the analysis falls behind, the most redundant ones are dropped, never the last one.
3. **Analysis** (`detect.rs`, `analyze` → `scene`): finds the face-up cards, straightens them to
   100×160 pixels and assigns each one a role: hand, trump card, opponent's card, my card.
4. **Recognition** (`mod.rs`, `read_briscola` → `recognize`): for each card, a score for each of
   the 40 Neapolitan cards.
5. **Game** (`assistant.rs`): readings become "sightings"; a card enters the game when it has
   been seen with enough certainty, otherwise the user is asked. The rules of briscola check that
   everything is consistent.

Between one analysis and the next, `track.rs` and `live.rs` follow the cards if the window moves
(for example, the overlay drawn over the cards).

---

## 2. Capture and the screenshot queue

- `capture.rs`: on Windows it photographs the monitors (`xcap`); elsewhere, and in the tests, it
  reads an image from `BRISMASTER_SCHERMO`.
- `screen.rs`: which monitor and which area to use (chosen by the user, or the whole monitor) and
  the diagnostic file to send when the reading goes wrong.
- `mod.rs`:
  - `CAPTURE_EVERY` = 16 ms;
  - `signature`/`difference`: a fingerprint of the screenshot, used to tell whether it changed;
    changes outside the card area (clocks, chat, ads) do not count;
  - `Frames`: at most `FRAMES` = 8 screenshots waiting, analysed in order. A site that closes a
    trick in half a second must not lose any card;
  - `shrink_factor`: large screens are scaled down so that a card has at least `MIN_CARD_PX` =
    110 pixels;
  - `Focus` / `focused_scene`: once the cards have been found, only the area where they are is
    analysed. Every `FULL_EVERY` = 24 readings the whole screen is analysed again. The learned
    area is saved in `tavolo.json` (`layout.rs`).

---

## 3. Finding the cards (`detect.rs`)

The idea: a face-up card is a light rectangle, with a white border, on a darker or coloured
background, and it looks like one of the reference cards.

### 3.1 Light areas

- `Paper`: what "white" means on this screen. Each channel is balanced, so a yellowish or bluish
  screen still has a white. There are more variants:
  - `Paper::of`: the normal white;
  - `Paper::darker`: darker whites, for cards dimmed by the site or slightly transparent;
  - `Paper::strict(249 / 243)`: only the whitest white, for tables almost as light as the cards.
- `blobs_*`: the connected light areas. Each one gets its smallest rotated rectangle
  (`min_area_rect`, with `hull` for the convex hull), represented by `RotRect`: centre, axes,
  half-width and half-height.
- `erode`: cards that touch (a fanned hand, a card stuck to a site panel) are separated by eroding
  more and more, until the pieces are the size of the cards already found.

### 3.2 Attempts (`analyze_once`)

The search is repeated with different whites until enough cards are found (`TRIES` = 7):

| attempt | what it uses |
|---|---|
| 0 | normal white (`analyze_inner` with `Paper::of`) |
| 1–3 | darker whites (`Paper::darker`) |
| 4, 5 | strict white 249 and 243 |
| 6 | cards drawn stretched (`analyze_shaped` with `card_shaped_wide`) |

It starts from the attempt that worked last time (`LAST_TRY`) and stops as soon as it has at least
3 cards, or as many as before. Some comparisons between attempts are already built in, for
example strict 249 against strict 243, and normal white against strict white when the hand had to
be split.

### 3.3 From rectangle to card

- `warp`: straightens the rectangle to 100×160 (`CARD_W`×`CARD_H`), head up; `balance` evens out
  the colours.
- `card_shaped`: card proportions, short side / long side between **0.52 and 0.76**.
  `card_shaped_wide` accepts the stretched cards drawn by some sites.
- `Try` / `finish` builds a `Found` with:
  - `score`: similarity to the most similar reference card (section 4);
  - `inside`: the fraction of the inside that has the table's colour (`table_inside`). A piece of
    table taken for a card has a lot of it;
  - `apart`: for each of the 4 sides, whether it stands out from what is outside (`sides_apart`);
  - `alone`: taken with the rules for unknown decks (section 3.5).
- `traits(card)` → `(border, ink)`: how much of the white border goes all the way round, and how
  much of the inside is drawn in colour.
- `Found::accepted(min)`: the normal rule. Similarity at least `ACCEPT` = **0.40** (or
  `ACCEPT_PARTIAL` = 0.50 for a card seen only in part), at least `MIN_INK` of drawing, and a
  border that goes nearly all the way round. A less certain border is allowed only with high
  similarity. Text, logos and panels can look like a card by chance, but they do not have the
  border and the colours.

### 3.4 Cards seen only in part

- The trump card under the deck, a card cut off by the edge of the screen or crossed by text: a
  light area **shorter** than a card. The app tries the possible positions of the whole card
  (`as_partial`) and compares only the visible band (`CardView::vis`, fractions of the height
  starting from the head).
- A fanned hand and cards stacked on top of each other: `ends`, `stacked`, `scan`/`scan_region`,
  `row_cards`, `uncover_fan`.
- Readings that are not certain are retried slightly rotated or shifted: `refine_pose`,
  `refine_pose_any`, `relocate_twin`.

### 3.5 Cards from decks the app does not know

A deck never seen before looks a little like the reference cards, but it is still a card: a
rectangle with card proportions, a white border all the way round and a coloured drawing inside.
Three rules make it possible to read it anyway; how much it is worth then comes from the
recognition (net and memory, section 5).

1. **A card on its own** (`Found::unknown_deck_card`, used by `as_full` and `accepted`). All of
   these must hold:
   - border ≥ 0.9, ink ≥ 0.05, `inside` < 0.12, seen whole;
   - apart from what surrounds it on at least 3 sides (`apart_alone`);
   - at least 12% coloured drawing (`drawn_color`, `MIN_COLORED`);
   - at least two distinct colours (`many_colors`), so not a white panel with a logo;
   - white background inside (`inner_paper` ≥ 0.2), or at least 3 hues (a face card full of
     colour).
2. **A card inside a group** (`unknown_in_group`, used by `unknown_pick` in `ends`/`stacked` and
   in `scan_region`): stricter on the border (≥ 0.95), but more lenient on the table colour inside
   (green clubs on a green cloth). It must be apart from the outside on two **opposite** sides
   (`apart_opposite`). In `ends`/`stacked`, if a card from an unknown deck was used and fewer than
   2 cards come out of it, everything is discarded: it is a single box, not a row of cards.
3. **Cards from outlines** (`edge_cards`): rectangles the size of the cards already found, with
   all four sides along a sharp edge of the image. They are used only on an unknown deck (below).

**Safeguards for the known decks** (the result of the 0.6.9 regressions):

- **Unknown deck or not.** Let `whole` be the whole cards found and `known` those with similarity
  ≥ `KNOWN_DECK` = 0.6. The deck counts as unknown if `whole == 0 || known*2 < whole`. On a known
  deck:
  - outline search is off;
  - every card with similarity < `ACCEPT` is removed.
- **Rereading without the unknown-deck rules** (`analyze` → `UNKNOWN_ON_KNOWN`). If the reading
  used a card taken with the unknown-deck rules, and the cards taken with the normal rules (seen
  partly too) are mostly from a known deck, the screen is read again with those rules switched
  off (`NO_UNKNOWN`). The reason: a wrong rectangle (a piece of table with the trump card in it)
  not only adds a false card, it also takes the place of the real cards. In the user's game u14
  it hid the hand and made the app read the wrong trump card.
- **Cut sides** (`finish`). If exactly 3 sides are apart and the fourth:
  - continues beyond, apart from the table as it is inside (`continues_beyond`: the long sides of
    a bigger card continue past the cut), or
  - does not have the card's light border just inside it (`paper_inside`: the rectangle includes
    the back of the deck),

  then it is not a card on its own, and it is marked as apart on no side.
- **Same size as the others.** A card taken on its own must have the length and width of the
  other cards found, within ±15%. Otherwise it is a piece of a card.

### 3.6 Who is who (`scene`)

- **Hand**: the lowest row of upright cards the size of the others, read left to right. It can
  span several rows stuck together (a hand laid out as a grid or a column), with at most 3 cards.
- **Trump card**: the card lying sideways, or partly covered next to the hand (under the deck).
  On some sites it lies upright, off to one side: the only face-up card outside the area of the
  played cards, halfway between the opponent's cards and the hand.
- **Played cards**: upright, not much smaller than the cards in the hand, roughly above the hand.
  The top one belongs to the opponent, the bottom one to me. The opponent's face-down cards, at
  the top, are used to split the table in half.
- **Grey screen**: if the cards come out almost colourless, the analysis is redone in grey mode
  (`set_gray_screen`), keeping only matches ≥ 0.85.

---

## 4. Comparison with the reference cards (`reference.rs`)

- **Reference decks** (`STYLES`), each a 10×4 sheet in `crates/app/assets/`:
  - `carte_napoletane_wiki.jpg`: Wikimedia, public domain. It is also the deck drawn in the app;
  - `carte_napoletane_classiche.jpg`: Wikimedia, public domain;
  - `carte_napoletane_brisva.jpg`: the deck of the site the user plays on, supplied and
    authorised by the user.
- **Thumbnail**: 16×26 pixels, each with brightness plus two colour differences (red–green,
  yellow–blue), compared with **normalised correlation**. That way how light, dark or saturated
  the screen is does not matter. Colour weighs `ALPHA` = 2 compared with brightness.
- **Crops and shifts**: every site draws a different white border around the card. The app tries
  several crops (`SX`, `SY`) and small shifts (`SHIFT`) on the most promising cards (`REFINE` =
  4), and keeps the best result. Below `HOPELESS` = 0.25 on the first pass it is not a card.
- **Colour signature** (`Hues`, weight `COLOR` = 0.2): how many red, yellow, green and blue
  points there are row by row. It tells suits apart even when the symbols are laid out the same
  way (seven of coins versus seven of cups).
- **Cards seen in part**: only the visible band is compared (`corr_masked`, `covered_rows`).
- **`quick_tops`**: a quick comparison to choose which way up the card is and to discard
  hopeless rectangles.
- **The site's style** (`site_style`): cards seen whole and recognised with certainty "vote" for
  a style (the last 12 votes count). Once the style is recognised, the other styles weigh 0.15
  less (`STYLE_PENALTY`).
- `CardView::sim`: the similarity, from −1 to 1, with each of the 40 cards.

---

## 5. From similarity to score (`mod.rs::recognize`)

Each card gets `scores[40]`, a log-likelihood-like score: higher means more likely. It is built
in this order.

1. **References** (`reference_scores`): `sim × SCALE` with `SCALE` = 8. If the first card is
   ≥ `EXACT` (0.95) and the second ≤ `EXACT_GAP` (0.88), the first gets +1.
2. **Neural network** (`nn.rs`), only if the card looks a little unlike the known decks. Let `top`
   be the maximum similarity; the weight is `w = clamp((NN_FROM − top)/0.25, 0, 1)` with
   `NN_FROM` = **0.65**, and the score is `s[c] += w · NN_WEIGHT · max(log p_net(c), −12)`. On the
   known decks (`top` ≥ 0.65) the net counts for nothing. With 0.85 it overrode correct readings
   of large cards from known decks (the "big cards" extreme game).
3. **Symbol count** (`pips.rs`): on a card seen whole, large enough (long side ≥ 110 px, short
   side ≥ 66) and with normal proportions, the symbols are counted. Cards with a different count
   lose `PIP_PENALTY` = 3. The count is used only if it agrees with at least one of the 4 most
   likely cards, otherwise it is the count that is wrong.
4. **Site memory** (`memory.rs`), section 7:
   - every remembered image with similarity ≥ `SAME` (0.9) adds `6 + (v − 0.9)·20` to its card;
   - **decisive memory**: if the closest image is ≥ `SURE` (0.95), with at least `SURE_GAP` (0.04)
     of lead over the second card, and the card looks little like the known decks
     (`top` < `DECISIVE_BELOW` = **0.75**), that card goes 6 points above all the others. On the
     known decks it is not used: an image remembered under the wrong name must not repeat the
     error (game u14).

`CardView::best(among)` gives the most likely card among the possible ones: in the hand, my cards
or the unseen ones; on the table, the card already there as well. It also gives the lead over the
second.

Note: one point of score is worth about 1/8 of similarity. A lead of 1.4 (`AUTO_MARGIN`) is about
0.175 of similarity.

---

## 6. The neural network (`nn.rs`, `scripts/rete-carte/`)

- **Input**: the straightened card shrunk to 40×64 (box filter), normalised card by card. Mean
  and deviation are removed, so the network does not depend on the screen's brightness.
- **Architecture**: 4 3×3 convolutions (16, 32, 48, 64 channels, ReLU, max-pool ×2 after the
  first three), adaptive pooling to 4×2, a 96-unit layer, then two heads: **suit** (4) and
  **rank** (10). The card's probability is `log p(suit) + log p(rank)`.
- **Weights**: `crates/app/assets/carte_rete.bin`, 32-bit little-endian numbers, layer by layer.
- **Training** (`scripts/rete-carte/allena.py`, see the instructions at the top of the file):
  - only the app's 3 reference decks (exported with the `esporta_riferimenti` test);
  - plus the same cards as the app straightens them on fake tables (`esporta_viste`);
  - heavy random changes: hue rotation, saturation, contrast, grey, thicker or thinner strokes,
    blur, pixel art, posterisation, covered patches, small rotations, upside-down cards.

  The goal is to recognise the shape and number of the suit symbols, not the drawing of one
  particular deck. The current weights come from 60 epochs with the exported views.
- **Measuring on unknown decks**: `allena.py` takes a folder of test decks, used only to print
  how many cards it recognises, never to train. Those decks stay local.
- To change the network, also change `Net::load` in `nn.rs` (the order of the layers and their
  sizes must match `esporta.py`).

---

## 7. Site memory (`memory.rs`)

- Every card **entered in the game** is stored as a 25×40 thumbnail, along with the band seen
  (`vis`). Partly covered cards are stored too: the trump card under the deck.
- At most 4 images per card (different lighting, rotations, visible parts). Very faint images
  (`faint`) are not stored.
- Comparison: normalised correlation on the band that both images show, trying 9 small shifts
  (`SHIFTS`).
- Saved in `Documenti\BrisMaster\carte_viste.json` (`VERSION` = 4). **Local only.**
- When a card is stored (`remember` in `assistant.rs`):
  - when the user confirms or corrects a card;
  - for the trump card, when it is read with certainty or confirmed;
  - for a card in the hand, only after it has been played and read the same on the table (see
    `to_learn`). A six read as a four and stored straight away would be read as a four forever.

  Cards accepted on their own after waiting (`auto_accept`) are not stored, because no one has
  confirmed them.
- Effect: on a new site the first game teaches the app its cards. On the 8 test decks the app
  has never seen, correct cards go from **64.2%** in the first game to **71.7%** after one game
  of learning (`mazzi_sconosciuti` test with `MEMORIA=1`, version 0.6.9).

---

## 8. From reading to game (`assistant.rs`)

- **Sightings** (`update`, `SIGHT_MARGIN` = 0.8): a reading counts as a sighting of a card if
  that card leads the second by at least 0.8 points. For each card the game keeps when and where
  it was seen. Hand, table and trump card are separate.
- **Smoothing** (`smooth`): only a still card counts, the same and in the same place as in the
  previous reading. Readings of a still card add up, so a passing wrong reading (a card sliding,
  turning over or lightening) does not change it.
- **Certainty** (`sure`): seen twice with certainty, or many times always the same.
- **Entering**: a card enters the game without asking if it leads by at least `AUTO_MARGIN` =
  1.4. Otherwise the app asks the user ("CHIEDE" in the test logs). If the same proposal stays
  the same for `AUTO_ACCEPT` = 6 readings it is accepted on its own (`auto_accept`), but its image
  is not stored. The user can reject a card (`reject`): it is no longer proposed for that step.
- **Rules as a check** (`screen_agrees`, `counts_ok`, `check_mismatch`):
  - as many cards in hand as the game has;
  - every card in the game is visible on the screen;
  - no card read with certainty that the game does not have;
  - on the table, the card from the game;
  - the deck goes down by two per trick.

  A different trump card, or a card already played that reappears in the hand, for a few seconds
  means another game: the user is told.
- **Recovery** when the game and the screen disagree:
  - `swap_misread`: one or two cards in the hand differ for several readings, so they were
    misread and get swapped;
  - `catch_up` / `rebuild`: the game fell behind, so the past readings are replayed following
    the rules until the game matches the screen again;
  - `resync` / `restart_empty`: the table was not visible for a while, so the game restarts from
    what is visible now. The opponent's cards in the meantime cannot be known, and the user is
    told;
  - `sync_trump`: the trump card, asking for confirmation if it is not certain;
  - **last resort** (`adopt_screen`, `ADOPT` = 30 ticks, about 10 s): if no recovery works, the
    game restarts from the screen, taking even the uncertain cards as valid, so the advice does
    not stay blocked. With 15 (about 5 s) it fired before the normal recoveries and before the
    question to the user, and in game u13 it kept restarting from a wrong game.

---

## 9. Data saved on the computer

| file | content | module |
|---|---|---|
| `Documenti\BrisMaster\carte_viste.json` | card images already seen on the site | `memory.rs` |
| `Documenti\BrisMaster\tavolo.json` | where the cards and the played cards sit on the screen | `layout.rs` |
| `Documenti\BrisMaster\Partite\...` | games recorded only if the user turns it on | `record.rs` |

None of these files leaves the computer.

---

## 10. Tests

### 10.1 Unit tests (fast, in CI)

```
cargo test --release -p brismaster
```

92 tests in version 0.6.9. They include fake tables (`synth.rs`) and screenshots of real sites
(`crates/app/testdata/`).

### 10.2 Benchmarks (slow, run by hand, `--ignored`)

All of them are run with `cargo test --release -p brismaster NAME -- --ignored --nocapture`.

| test | what it measures | how to run it |
|---|---|---|
| `briscola_extreme_games` (assistant.rs) | 17 whole games on fake tables pushed to the extremes: small, big, narrow, wide cards, fans, light or dark table, light, grey, blurry text… | `BRIS_STRESS_CASO=strette` for a single case |
| `partite_registrate` (assistant.rs) | games recorded from a real table, one screenshot every 350 ms, with the real situation. Prints confirmations, cards written by hand and errors | `PARTITE=folder`, `VERBOSO=1` prints every reading |
| `mazzi_sconosciuti` (vision/mod.rs) | percentage of correct cards on never-seen decks, on many tables in two orders | `MAZZI=folder` (one subfolder per deck, `00.png`..`39.png`), `MEMORIA=1` for the first game teaches and the second measures |
| `banco_siti`, `banco`, `banco_partite` | tables and games on fake sites | |
| `diagnosi_immagine` | what the reading finds in any screenshot | `IMG=screenshot.png` |
| `prova_rete` | the network's 3 most likely cards for a card | `RETE=card.png` |
| `esporta_riferimenti`, `esporta_viste` | data for training the network | section 6 |

### 10.3 Switches to measure a single rule

They only work inside the tests (`cfg!(test)`). Each one switches off a rule, so you can see how
much it is worth:

| variable | rule switched off |
|---|---|
| `SENZA_SCONOSCIUTA`, `SENZA_NELGRUPPO`, `SENZA_BORDI` | the three unknown-deck rules |
| `SENZA_LATI`, `SENZA_COLORE`, `SENZA_GRUPPI` | the side check, the colour check, the "two colours" check |
| `SENZA_PEZZI`, `SENZA_MISURA`, `SENZA_RILETTURA` | the three safeguards for the known decks |
| `SENZA_NOTO`, `SENZA_NOTO_SCONOSCIUTE` | treats every deck as unknown, or keeps the unknown-deck cards on known decks too |
| `SENZA_DECISIVA`, `SENZA_MEMORIA` | decisive memory, all memory |
| `SENZA_SIMBOLI`, `SENZA_STILE` | symbol count, the site's style |

Other variables:

- `NN_DA` and `NN_K`: from what similarity downwards the network counts, and how much.
- `ADOPT`: the last-resort delay, in ticks.

Diagnostics: `SCARTI=1` prints the rejected rectangles and why, `VEDI_RILETTURA=1` prints when
the reread kicks in, `TEMPI=1` prints the times of the attempts, `NN_LOG=1` prints the network's
weight.

### 10.4 How to compare two versions properly

- Every **git worktree** must have its own `CARGO_TARGET_DIR`. The test binary's name
  (`brismaster-<hash>`) **does not depend on the folder**, so two working copies sharing the same
  `target` overwrite each other's binaries. In 0.6.9 this produced a "comparison" between 0.6.8
  and 0.6.8 with identical results.
- Check which code the binary comes from:
  `strings BINARY | grep crates/app` shows the folder it was compiled from.
- Same data, same variables, and compare game by game (not only the totals). Example: on the 20
  recorded games the totals of 0.6.9 and 0.6.8 match, but this was checked game by game.

---

## 11. Results of version 0.6.9 (compared with 0.6.8)

| benchmark | 0.6.8 | 0.6.9 |
|---|---|---|
| 20 user games (to the end / perfect / confirmations / cards by hand) | 20 / 4 / 31 / 16 | 20 / 4 / 31 / 16 (identical game by game) |
| 17 extreme games | 17/17 | 17/17 |
| site scraping (`spoglio`) / display | 20/20 · 19/20 perfect | 20/20 · 19/20 |
| 8 never-seen decks, first time | (the test does not exist) | 64.2% correct cards |
| 8 never-seen decks, after one game | (the test does not exist) | 71.7% correct cards |

---

## 12. Known problems and lessons

- **Rules for unknown decks on known decks.** Every rule that accepts cards resembling nothing
  known can turn a piece of a card, the back of the deck, or a piece of table with the trump card
  in it, into a card. The cases of 0.6.9:
  - wide cards split in half;
  - the deck together with the trump card;
  - a rectangle at the start of the game that hid the hand.

  Before adding such a rule: limit it to the unknown decks, and run `briscola_extreme_games` and
  `partite_registrate`.
- **Narrower proportions** (short/long down to 0.46) broke the "narrow cards" game: a narrow
  rectangle is more often a piece of a card. They have been put back to 0.52.
- **Network and memory are strong**: they decide the card even against the references. That is
  why they count only when the references have little to say (`NN_FROM`, `DECISIVE_BELOW`).
- **The decks that remain hard**: Piacentine cards and decks with a very different design
  (`aflorj` and `piacentine_marini` among the test decks) have very low first-time recognition;
  memory helps a lot there.
- **A recovery that fires too early does harm.** The recoveries must leave time for normal
  confirmations: a question to the user is better than a wrong restart.

---

## 13. Where to start for a change

| I want to… | where to look |
|---|---|
| find cards on a new kind of table | `detect.rs`: `Paper`, `analyze_once` (attempts), `analyze_shaped` |
| read a deck from a new site | add a reference sheet (`reference.rs`, `STYLES`), or let memory and network do the work |
| retrain the network | `scripts/rete-carte/allena.py`, then `esporta.py`; measure with `mazzi_sconosciuti` |
| change who is hand / trump / table | `detect.rs::scene` |
| change when the app asks for confirmation | `assistant.rs`: `AUTO_MARGIN`, `AUTO_ACCEPT`, `SIGHT_MARGIN` |
| change the recoveries | `assistant.rs`: `STALL`, `BEHIND`, `RESTART`, `ADOPT` and the `resync*`/`catch_up`/`rebuild` functions |

Before publishing: unit tests, `briscola_extreme_games`, `partite_registrate` on the recorded
games compared with the previous version, and `mazzi_sconosciuti` with and without `MEMORIA`.
