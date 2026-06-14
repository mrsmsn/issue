## 概要

- ローカルで完結するissue管理CLI.
- OSSとして開発する
- コマンドのオプションや使い方は`gh issue` にできる限り寄せる（github固有の機能は対象外とする）
- `issue` コマンドを実行することで、$PROJECT_ROOT/issue にissueをfrontmatter付きのmarkdownを対話形式で作成する。
- GithubのIssueと互換性あり（export、import機能）

## 利用想定

- 個人開発者
- チーム開発
- OSS開発

## frontmatter

id: number # 1 始まりの連番整数。ゼロ埋めしない。並びは数値ソートで担保（採番ルールは #1 で決定）
title: string
status: open, closed, in-progress, wontfix
type: string
created: 2026-06-14
updated: 2026-06-14
labels: []
related: []

## 構想

- 将来的にTUIアプリLazyissueを作成する（イメージ的にはLazygitに近しい）。別Repoか？

## 考えきれてないこと

- 競合のgit-bug/git-bug との差別化
- ~~id の具体的な値のルール~~ → **決定: 1 始まりの連番整数・ゼロ埋めなし・並びは数値ソートで担保（#1）。**
  （桁あふれは整数＋数値ソートで起きないため「NNNN→NNNNN」の桁拡張は論点に含めない）
- ~~チーム開発でのマージ衝突運用~~ → **決定: 楽観的採番（ローカルで `max id + 1`）。
  ブランチ並行起票で id が重複しても後から振り直さない（id は不変）。重複は検出して警告する
  （`issue lint` 相当 / pre-commit hook / CI）。renumber・`issue rebase` は採用しない
  — id を可変にすると `#N` 参照・`related`・ブランチ名・コミット参照が壊れるため（#1）。**
- 明示的に指示せずとも、issueを使用したワークフローであることをコーディングエージェントが気づきやすい仕組みにしたい

