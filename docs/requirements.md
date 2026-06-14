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

id: string
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
- id: stringの具体的な値のルール. 連番が望ましいがチーム開発で使用すると分散型であるが故にコンフリクトリスクに伴うUX低下が発生するので回避したい
- 明示的に指示せずとも、issueを使用したワークフローであることをコーディングエージェントが気づきやすい仕組みにしたい

