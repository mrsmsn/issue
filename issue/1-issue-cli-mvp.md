---
id: 1
title: "issue CLI: ローカル完結 issue 管理ツール (MVP エピック)"
status: open
priority: P1
created: 2026-06-14
updated: 2026-06-14
labels: [cli, mvp, epic]
---

## 背景 / なぜ

個人 / チーム / OSS 開発で、issue を外部サービス（GitHub 等）に依存せず、
リポジトリ内で完結して管理したい。issue がリポジトリと同じ Git 履歴に乗ることで、
オフラインでも扱え、コードと一緒にレビュー・バージョン管理でき、
コーディングエージェントが追加の認証や API 無しにそのまま読み書きできる。

操作感は `gh issue` を知っていれば学習コストなく使えることを狙う
（オプション・サブコマンド体系を可能な限り `gh issue` に寄せる。GitHub 固有機能は対象外）。

## 現状 (As-is)

- issue 管理は CLI として存在しない。`docs/requirements.md` に構想のみがある。
- リポジトリ内で issue を起票・一覧・参照する標準的な手段がなく、
  起票は手作業で Markdown を書くしかない。

## あるべき姿 (To-be)

`issue` コマンド一式で、ローカルの `$PROJECT_ROOT/issue/` に
frontmatter 付き Markdown として issue を作成・一覧・参照できる。
将来的に GitHub との export/import 互換、編集 / クローズ、TUI を備える。

本エピックは全体像を俯瞰し、**MVP（init / create / list / view）** の達成を
ゴールとする。各サブタスクの詳細な受け入れ条件は個別 issue で起票する。

## 受け入れ条件

MVP スコープ（init / create / list / view）として、以下が満たされること:

- [x] `issue init` を実行すると、プロジェクト root 直下に `issue/` ディレクトリが用意され、
      再実行しても既存の issue ファイルを破壊しない
- [x] `issue create` を引数なしで実行すると対話形式で title / type / labels 等を尋ね、
      `issue/N-slug.md` が frontmatter 付きで生成される
- [x] 生成される id は `1` から始まる連番整数（ゼロ埋めしない）で、ファイル名の `N` と一致する。
      一覧の並び順は id の数値順で担保し、ファイル名の辞書順には依存しない
- [x] 生成された frontmatter は `id / title / status / type / created / updated / labels / related`
      を含み、`status` は `open`、`created` / `updated` は実行日
- [x] `issue list` で起票済み issue が一覧表示され、`status` と `label` で絞り込める
- [x] `issue view <id>` で該当 issue の frontmatter と本文が表示される
- [x] 各コマンドのオプション体系が、対応する `gh issue`（create / list / view）に
      可能な限り一致する（GitHub 固有機能は除く）
- [x] `issue --help` および各サブコマンドの `--help` が使い方を表示する

## サブタスク

詳細な受け入れ条件は各サブタスクを個別 issue として起票する。

MVP:

- [x] `issue init` — `issue/` 初期化、README 生成、冪等な再実行
- [x] `issue create`（対話）— 対話形式での frontmatter 付き Markdown 生成、連番採番（非対話フラグも対応）
- [x] `issue list` — 一覧表示、status / label フィルタ
- [x] `issue view` — 指定 id の表示

後続（MVP の後）:

- [x] `issue edit` / `close` / `reopen` — status 変更・本文編集（実装済み。frontmatter を
      行単位でサージカル編集し、未知キー・本文・順序を保持。ファイル名はリネームしない）
- [x] `issue export`（→ GitHub）— GitHub Issues 互換 JSON 出力（REST API 形状、ADR 0004）
- [x] `issue import`（← GitHub）— GitHub Issues からの取り込み（snake/camel 両対応、id 衝突は非破壊リマップ）
- [x] `issue lint` — 重複 id の検出・警告（実装済み。pre-commit hook / CI 連携は今後）
- [ ] git-bug との差別化・ポジショニング整理（spike）
- [ ] コーディングエージェントが本ワークフローに気づきやすくする仕組み（spike）
- [ ] TUI「LazyIssue」（別 repo 候補として将来検討）

## スコープ外

- GitHub 固有機能（PR 連携、ラベル色、マイルストーン、Projects 等）
- TUI（LazyIssue）— 本エピックでは対象外、別 repo 候補として将来検討
- export / import — MVP の後続
- edit / close / reopen — MVP の後続（後続 issue で起票）

## 関連

- `docs/requirements.md` — 構想・利用想定・frontmatter 案
- `docs/adr/0001-issue-id-is-a-plain-integer.md` — id を連番整数（ゼロ埋めなし）にした決定の記録
- `docs/adr/0002-implement-core-in-rust.md` — Go/Rust ベンチで Rust 採用した記録
- `docs/adr/0003-github-aligned-frontmatter-schema.md` — status を open/closed のみにし type/related を廃止した記録
- 競合: git-bug（差別化は未解決の論点）

## 決定事項

- **id は `1` 始まりの連番整数。ゼロ埋めしない**（`1`, `2`, …, `12345`）。
  一覧の並び順は id を**数値**としてソートして担保し、ファイル名の辞書順には依存しない
  （固定幅ゼロ埋めの桁あふれ問題を持たない）
- 採番は楽観的（ローカルで `max(existing id) + 1`）。ブランチ並行起票で id が重複しても
  **後から振り直さない（id は不変）**。renumber / `issue rebase` は採用しない
  — id を可変にすると `#N` 参照・`related`・ブランチ名・コミットメッセージの参照が壊れるため
- 重複 id は**検出して警告する**（`issue lint` 相当 / pre-commit hook / CI）。検出のみで自動修正はしない
  （git は別ファイルを論理衝突なくマージするため、重複検出はツール側で持つ）
- 起票の進め方: 本 issue をエピックとし、各サブタスクは個別 issue で詳細化する
- MVP = `init` / `create`（対話）/ `list` / `view`
- 操作感は `gh issue` に寄せる。GitHub 固有機能は対象外

## 未解決の論点

- git-bug との差別化・ポジショニング
- コーディングエージェントが本 issue ワークフローに気づきやすくする仕組み
- import 時の id 整合の運用（衝突リマップで番号が変わる点の周知）
