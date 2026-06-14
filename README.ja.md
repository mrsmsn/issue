# issue

*[English](README.md) | 日本語*

**リポジトリの中に住む、ローカルファーストな issue トラッカー。** issue は `issue/`
ディレクトリ配下の frontmatter 付き Markdown ファイルとして保存されます — 1 issue
1 ファイルで、追跡対象のコードと一緒にバージョン管理されます。サーバーもリモートバック
エンドもアカウントも不要。あるのはファイルだけです。

CLI は可能な限り `gh issue` に揃えてあるので、手に馴染んだ操作がそのまま使えます。
[`git-bug`](https://github.com/git-bug/git-bug) のようなツールとの違いは、issue が
**手で編集でき diff も取れる Markdown** であること、そして JSON の `export`/`import`
で GitHub Issues と相互変換できることです。

```sh
brew install mrsmsn/tap/issue
issue init
issue create --title "Fix the flaky test" --label bug
issue list
```

## 特長

- **ローカルファースト & オフライン。** すべてが作業ツリー内のファイル。ネットワーク不要で動作します。
- **ファイルが契約。** 各 issue は 1 つの `<id>-<slug>.md`。GitHub 準拠の小さな frontmatter
  スキーマで、ツールがなくても読み書きできます。
- **`gh issue` 準拠の CLI。** コマンド・フラグ・挙動は既定で `gh issue` に従います。
- **GitHub 相互変換。** `export`/`import` は GitHub の REST/GraphQL issue JSON を扱い、
  id を非破壊的に調整します。
- **`lazyissue` TUI。** 閲覧・フィルタ・編集ができる Lazygit 風のターミナル UI。
- **シェル補完** を bash/zsh/fish に対応。id/label/status を動的に補完します。
- **小さく依存ゼロのコア。** `issue` CLI は std のみの Rust（`serde`/`clap` なし）。
  オフラインでビルドでき、サイズも小さく保たれます。
- **エージェントに優しい。** `issue/` ディレクトリを持つリポジトリは、自身のワークフローを
  自己申告します — コーディングエージェントは issue ファイルを読んでそのまま着手できます。

## インストール

### Homebrew（推奨）

```sh
brew install mrsmsn/tap/issue
```

アーキテクチャ別のビルド済みバイナリ（コンパイル不要）を取得し、`issue` と `lazyissue`
の両方、さらにシェル補完までインストールします — 追加設定なしで `issue <Tab>` が効きます。

### ソースから

Rust ツールチェインが必要です。

```sh
cargo build --release
# -> target/release/issue
# -> target/release/lazyissue
```

## クイックスタート

```sh
issue init                      # ./issue ディレクトリを作成（冪等）

issue create                    # 対話: title/labels を stdin で入力
issue create --title "Bug X" --label bug --label p1 --body "Steps to reproduce…"

issue list                      # 全 issue
issue list --status open --label bug
issue view 1                    # 単一 issue ファイルを表示

issue edit 1 --add-label needs-repro --status closed
issue close 1
issue reopen 1
```

## コマンド

| コマンド | 説明 |
| --- | --- |
| `issue init` | issue ディレクトリを作成（冪等。既存ファイルは上書きしない）。 |
| `issue create` | issue を作成 — 対話、またはフラグ経由。 |
| `issue list` | issue を一覧（タブ区切り）。フィルタ任意。 |
| `issue view <id>` | 単一 issue ファイルを表示。 |
| `issue edit <id>` | フィールドをその場で編集（title/status/labels/body）。 |
| `issue close <id>` | status を `closed` にし `updated` を更新。 |
| `issue reopen <id>` | status を `open` にし `updated` を更新。 |
| `issue lint` | id の重複を検出。あれば非ゼロ終了。 |
| `issue export` | 全 issue を GitHub 形式の JSON 配列で出力（stdout）。 |
| `issue import [FILE]` | GitHub 形式 JSON から import（ファイル or stdin）。 |
| `issue completions <shell>` | 補完スクリプトを出力（`bash`/`zsh`/`fish`）。 |
| `issue version` | バージョンを表示（`--version`/`-V` も可）。 |

`issue --help` / `-h` でトップレベルのヘルプ、`issue <command> --help` で各コマンド固有の
オプションが見られます。主なフラグ:

- **`create`** — `--title T`、`--label L`（複数指定可）、`--status open|closed`
  （既定 `open`）、`--body TEXT`。フラグなしなら対話入力。
- **`edit`** — `--title T`、`--status S`、`--add-label L` / `--remove-label L`
  （複数指定可）、`--body TEXT`。フィールドをその場で更新します — ファイル名はリネームしません。
- **`list`** — `--status S`、`--label L` でフィルタ。出力は
  `<id>\t<status>\t<title>\t<labels>`。

## issue ファイル形式

各 issue は `issue/<id>-<slug>.md`。整数の `id` が安定した識別子で、slug は飾りです。
issue ディレクトリは `$ISSUE_DIR` が設定されていればそれ、なければ `./issue`。

```markdown
---
id: 1
title: Issue CLI MVP
status: open
created: 2026-06-14
updated: 2026-06-14
labels: [feature, epic]
---

## Background
なぜこの issue が存在するか…

## Related
- #2
```

スキーマ（GitHub 準拠 — [ADR 0003](docs/adr/0003-github-aligned-frontmatter-schema.md)）:

| key | 意味 |
| --- | --- |
| `id` | 1 始まりの整数。ゼロ埋めしない。不変 — 再採番しない。 |
| `title` | 1 行サマリ。 |
| `status` | `open` または `closed`。 |
| `created` / `updated` | `YYYY-MM-DD`。 |
| `labels` | インラインリスト（例 `[bug, p1]`）。分類は label で行う。 |

意図的に **`type` フィールドは持ちません**（分類は label で）。また **`related`
フィールドも持ちません**（関連は本文でクロスリファレンスする。例: `## Related`
セクションに `- #N` リンク）。ファイル内の未知のキー（`priority:` など）は編集時に保持され、
捨てられません。id は `max(existing) + 1` で楽観的に採番され、ブランチ間の衝突は
自動再採番せず `issue lint` で検出します（[ADR 0001](docs/adr/0001-issue-id-is-a-plain-integer.md)）。

## GitHub 相互変換

GitHub Issues の JSON 形式で相互変換できます（[ADR 0004](docs/adr/0004-github-json-interchange-for-export-import.md)）:

```sh
issue export > issues.json          # id 昇順の pretty JSON 配列
issue import issues.json            # または: gh issue list --json … | issue import
```

`import` は寛容です — REST（`created_at`）でも GraphQL（`createdAt`）のキー形でも、
label が文字列でも `{ "name": … }` オブジェクトでも受け付けます。既存ファイルは
決して上書きしません: ソースの issue 番号が空いていればそれを使い、埋まっていれば新しい
id を割り当てて、その対応（`imported #N (was #M)`）を報告します。

## lazyissue（TUI）

[ratatui](https://ratatui.rs) 製の Lazygit 風ターミナル UI。同じ `issue/` ディレクトリに対し、
**Filters** / **Issues** / **Detail** の 3 ペインで操作します。

```sh
lazyissue
```

| キー | 操作 |
| --- | --- |
| `j` / `k`、`↓` / `↑` | 下 / 上に移動 |
| `g` / `G` | 先頭 / 末尾へ |
| `Ctrl-d` / `Ctrl-u` | 半ページ下 / 上 |
| `Tab` / `BackTab`、`h` / `l` | ペイン切り替え |
| `Enter` / `Space` | ハイライト中のフィルタを適用（Filters ペイン） |
| `n` | 新規 issue |
| `e` | issue を編集 |
| `b` | issue 本文を `$EDITOR` で編集 |
| `c` / `o` | 選択中の issue を close / reopen |
| `/` | title で検索（大文字小文字を無視） |
| `R` | ディスクから再読み込み |
| `?` | ヘルプの表示切り替え |
| `q` / `Esc` | 終了 |

## シェル補完

Homebrew は補完を自動でインストールします。手動で設定する場合は、次の出力を source します:

```sh
issue completions bash   # または: zsh, fish
```

補完は動的です: 入力に応じて既存の issue id・label・status を候補に出します。

## 開発

Cargo ワークスペースです（[ADR 0005](docs/adr/0005-tui-lazyissue-and-workspace-split.md)）:

- **`crates/core`**（`issue-core`）— 純粋ロジック、ストレージ、create/edit/close の
  サービス層、自前の JSON パーサ。**外部依存ゼロ**（std のみ）。
- **`crates/cli`**（`issue`）— CLI シェル。`issue-core` のみに依存するので、
  std のみ・オフラインを保ちます。
- **`crates/tui`**（`lazyissue`）— TUI。依存を持つことを許された唯一のクレート
  （ratatui/crossterm/notify）。

```sh
cargo test --workspace          # 全テストを実行
cargo build -p issue --offline  # CLI が依存ゼロであることを証明
cargo run -p lazyissue          # TUI を起動（実ターミナルが必要）
```

`issue-core` と `issue` CLI を std のみに保つのは厳格なルールです — 新しい ADR なしに
クレートを追加しないでください（理由は [ADR 0002](docs/adr/0002-implement-core-in-rust.md)）。
設計判断は [`docs/adr/`](docs/adr/)、コントリビューター向けガイドは
[`CLAUDE.md`](CLAUDE.md) にあります。

### アーキテクチャ決定（ADR）

- [ADR 0001](docs/adr/0001-issue-id-is-a-plain-integer.md) — issue id は素の整数
- [ADR 0002](docs/adr/0002-implement-core-in-rust.md) — コアを Rust で実装
- [ADR 0003](docs/adr/0003-github-aligned-frontmatter-schema.md) — GitHub 準拠の frontmatter スキーマ
- [ADR 0004](docs/adr/0004-github-json-interchange-for-export-import.md) — export/import の GitHub JSON 相互変換
- [ADR 0005](docs/adr/0005-tui-lazyissue-and-workspace-split.md) — `lazyissue` TUI とワークスペース分割
- [ADR 0006](docs/adr/0006-prebuilt-binary-homebrew-distribution.md) — ビルド済みバイナリの Homebrew 配布

## ライセンス

[MIT](LICENSE)。
