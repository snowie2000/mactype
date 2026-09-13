MacType
========================

Windows のフォントレンダリングを改善。

最新ビルド
------------------

[ダウンロード](https://github.com/snowie2000/mactype/releases/latest)

公式サイト
------------------

MacType 公式サイト: 

http://www.mactype.net

What's new?
------------------

- Win11 の互換性
- CET の互換性
- 更新された FreeType
- カラーフォントのサポート :sunglasses:
- 新しいインストーラー
- 多くのバグ修正
- マルチモニターのサポートを更新
- サービスモードでエクスプローラーのトレイアプリを傍受
- ダイアクリティカルマークの調整
- EasyHook を更新
- トレイモードで CPU の低負荷設定
- [しらいと](http://silight.hatenablog.jp)氏による DirectWrite のサポートの向上
- DirectWrite パラメータの個別調整 
- GT Wang による繁体字中国語の大幅な改善
- 英語訳の改善
- 韓国語訳の追加、조현희に感謝
- 多言語システムの改善

寄付
------------------

MacType は現在寄付を受付中です。

http://www.mactype.net にアクセスして右下の隅にご注目ください :heart:

ご支援ありがとうございます! 寄付金はサーバーの維持費、更新の継続、そしてコーヒーの購入に役立てられます :coffee:

既知の問題
---------------

- 更新をする前にプロファイルをバックアップしてください。

- 中国語 (繁体字/簡体字) と英語のみが完全にローカライズされており、言語ファイルに文字列の欠落があります。MacType Tuner で一部のオプションに欠落がある可能性があります。翻訳にご協力ください。

- MacType-patch を MacType 公式ビルドと一緒に使用したい場合は、プロファイルに｢DirectWrite=0｣を追加することを忘れないでください。この操作を忘れると動作に問題が発生します。

- Office 2013 は DirectWrite または GDI を使用していません (独自のカスタムレンダリングを使用しています)。そのため、Office 2013 で MacType は動作しません。これが気になる方は、GDI を使用している Office 2010 か DirectWrite を使用している Office 2016 以降を使用してください。 

- WPS には MacType を自動的に **アンロード** する防御機能が組み込まれています。最新のバージョンでは、wmjordan の協力により[こちら](https://github.com/snowie2000/mactype/wiki/WPS)の回避策が用意されています。

レジストリモードを戻す方法
-------------

Windows 10 以降ではウィザードを使用したレジストリモードの有効化はできなくなりました。 

レジストリモードを手動で有効化する方法については、[Wiki](https://github.com/snowie2000/mactype/wiki/Enable-registry-mode-manually) に詳しいガイドがあります。<br>
アクセスする前にスクリュードライバーを準備してくださいね。

ビルドのやり方
-------------

ビルドについては[ドキュメント](https://github.com/snowie2000/mactype/blob/directwrite/doc/HOWTOBUILD.md)をご確認ください。

