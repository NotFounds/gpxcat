# gpxcat

[![Test](https://github.com/notfounds/gpxcat/actions/workflows/test.yml/badge.svg)](https://github.com/notfounds/gpxcat/actions/workflows/test.yml)
[![Release](https://github.com/notfounds/gpxcat/actions/workflows/release.yml/badge.svg)](https://github.com/notfounds/gpxcat/actions/workflows/release.yml)

`gpxcat` は、GPX ファイル 1 つを読み、ハイキング・ランニング・サイクリングなどの記録の距離・標高・時間を、人が読むテキストか機械が読む JSON で標準出力に出す Rust 製の CLI ツールです。

## インストール

### プリビルドバイナリ（推奨）

[GitHub Releases](https://github.com/notfounds/gpxcat/releases) から最新のバイナリをダウンロードできます。

```bash
# Linux (x86_64)
curl -L https://github.com/notfounds/gpxcat/releases/latest/download/gpxcat-linux-x86_64.tar.gz | tar xz

# macOS (Apple Silicon)
curl -L https://github.com/notfounds/gpxcat/releases/latest/download/gpxcat-macos-aarch64.tar.gz | tar xz

# macOS (Intel)
curl -L https://github.com/notfounds/gpxcat/releases/latest/download/gpxcat-macos-x86_64.tar.gz | tar xz

# Windows
# gpxcat-windows-x86_64.zip をダウンロードして展開
```

### ソースからビルド

```bash
git clone https://github.com/notfounds/gpxcat.git
cd gpxcat
cargo build --release

# バイナリは target/release/gpxcat に作成されます
```

## 使い方

```bash
gpxcat track.gpx
```

```
合成テスト山行 (北峰・南峰)
2026-08-15 06:00 → 2026-08-17 12:59 (JST)  2泊3日

  距離          8.9 km
  累積標高      ↑ 223 m   ↓ 513 m
  最高点        1,721 m   08-17 05:15  (36.40500, 137.50400)
  最低点        1,210 m   08-17 12:59  (36.35000, 137.45000)
  経過時間      54h 59m
  行動時間      7h 39m   ※停止判定: 2分間の平均速度が 0.2 m/s 未満
  起点と終点    7,138 m 離れている（片道）

日別
  日付   開始   終了   点数    距離      ↑      ↓     最高
  08-15  06:00  06:30     5  0.4 km  150 m    0 m  1,650 m
  08-17  05:00  12:59     6  8.5 km   73 m  511 m  1,721 m
```

### JSON で出力する

```bash
gpxcat --format json track.gpx
```

JSON の数値は丸めずに出します。時刻は `--tz` のオフセット付き ISO 8601 で出します。

### 1 km ごとの区間を表示する

```bash
gpxcat --detail track.gpx
```

日別の表の後ろに、1 km ごとの区間の表を足します。ペースと速度は、区間の中の停止を除いた行動時間から計算します。

```
区間（1 km ごと。ペースと速度は行動時間から計算）
  区間         開始    距離  行動時間     ペース      速度      ↑      ↓
     1  05-04 08:30  1.0 km    0h 25m  24'59"/km  2.4 km/h  410 m    0 m
     2  05-04 08:55  1.0 km    0h 25m  25'38"/km  2.3 km/h    0 m  405 m
```

- 区間は、前の区間の最後の点から距離が 1 km 以上になった最初の点で区切ります。そのため区間の距離は 1 km ちょうどにはならず、最後の区間は 1 km 未満になることがあります
- 記録が途切れた区間（日をまたぐ移動など）も、そのまま 1 つの区間に入ります
- JSON では `splits` として出力します。`speed_mps` と `pace_s_per_km` は行動時間が 0 の区間では `null` です

### タイムゾーンを指定する

```bash
gpxcat --tz Europe/Zurich track.gpx
```

## オプション

| オプション | 説明 |
|-----------|------|
| `--format <text\|json>` | 出力形式（既定: `text`） |
| `--tz <IANA名>` | 表示に使うタイムゾーン（既定: `Asia/Tokyo`） |
| `--detail` | 1 km ごとの区間のペースと速度も出力する |

終了コードは、成功で 0、ファイルの読み込みや解析の失敗で 1、引数の誤りで 2 です。

## 集計の定義

隣り合う 2 点の組を「区間」と呼びます。

| 項目 | 定義 |
|------|------|
| 距離 | 全区間の haversine 距離（地球半径 6,371,000 m）の合計 |
| 累積標高 ↑ / ↓ | 全区間の標高差のうち、正の値の合計と負の値の絶対値の合計。平滑化やしきい値はかけない |
| 最高点 / 最低点 | 標高が最大 / 最小の点。同じ値の点が複数あるときは先に出てくる点 |
| 経過時間 | 最後の点の時刻 − 最初の点の時刻 |
| 行動時間 | 経過時間 − 停止した時間 |
| 停止した時間 | 下記の「停止の判定」で停止とみなした幅を、重なりをまとめて合計した時間 |
| 起点と終点 | 最初の点と最後の点の haversine 距離。200 m 以下なら「往復またはループ」、超えたら「片道」 |
| 泊数 | `--tz` での最後の点の日付 − 最初の点の日付。0 なら「日帰り」 |

### 停止の判定

各点から 120 秒以上後の最初の点までを 1 つの幅とし、幅の両端の直線距離 ÷ 時間差が 0.2 m/s 未満なら、その幅を停止とみなします。

- 隣り合う 2 点ではなく時間の幅で判定するので、1 秒ごとの記録でも数分ごとの記録でも同じように停止を検出できます。GPS の位置のぶれも幅の分だけ平均されます
- 0.2 m/s は 2 分で 24 m しか進まない速さです。活動の種類ごとの速さではなく「ほぼその場から動いていない」ことを表すので、歩き・走り・自転車のどれにも同じ値を使います
- 停止をまたぐ幅は、幅の中の移動が 24 m 未満なら幅全体が停止になります。そのため停止の前後の数秒〜数十秒（速く動いているほど短い）も停止に含まれます
- 行動時間はこの値しだいで大きく変わるため、text と json のどちらにも判定の値を出力します

### 日別

日別の距離・累積標高は、両端の点が同じ日に属する区間だけから求めます。日付をまたぐ区間はどの日にも入らないため、日別の合計が全体の値と一致しないことがあります。

## 入力

`trk > trkseg > trkpt` を、ファイルに出てくる順に 1 本の点列として読みます。各点の `lat`・`lon`・`ele`・`time` は必須で、欠けた点があれば何番目の点かを標準エラーに出して終了します。`time` にオフセットが無いときは UTC として解釈します。見出しには最初の `trk/name` を使い、無ければファイル名を使います。

## ライセンス

MIT License © 2026 Iori Ikeda

## 貢献

バグ報告や機能リクエストは、GitHubのIssueでお願いします。
