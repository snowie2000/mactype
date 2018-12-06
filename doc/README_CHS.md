请以首页英文文档为准，中文文档更新可能较慢。

MacType
========================

Better font rendering for Windows.

最新beta版本
------------------

[1.2018.10.19-beta4](https://github.com/snowie2000/mactype/releases/tag/v1.2018.10.19-beta4)

请阅读以下说明了解使用方法和注意事项

MacType官网
------------------

MacType 官方网站（下载点为最近的稳定版，较开发版为老） 

http://www.mactype.net

更新日志
------------------

- Win10 兼容
- 更新 FreeType 到 git 版本 0c4feb72cf976f63d4bf62436bc48f190d0e0c28
- 支持彩色字体 :sunglasses:
- 全新安装程序
- 大量bug修正
- 更好的多显示器支持
- 托盘程序现在能正确加载到explorer.exe了
- 注音字体支持优化
- EasyHook更新
- 托盘模式现在不会再占用大量cpu了
- 更新的DirectWrite支持，感谢しらいと[http://silight.hatenablog.jp] 和 @extratype
- DirectWrite参数支持独立调节
- GT Wang提供更好的繁体中文本地化
- 英语本地化翻译优化
- 支持韩语，感谢조현희
- 多国语言支持优化
- (暂不包含Infinality补丁，该补丁仍不稳定)

Donation
------------------

MacType now accepts donations. 

Please visit http://www.mactype.net and keep an eye on the bottom right corner :heart:

Thank you for your support! Your donations will keep the server running, keep me updating, and buy more coffees :coffee:

Known issues
---------------

- Please backup your profiles before upgrading!

- Only Chinese simplified/Traditional and English are fully localized, some options may missing in MacType Tuner due to the strings missing in the language file. You can help with translations!

- If you want to use MacType-patch together with MacType official release, remember to add DirectWrite=0 to your profile or you will have mysterious problems

- If you're running 64 bit Windows, antimalware/antivirus software may conflict with MacType, because it sees MacType trying to modify running software. One possible workaround is to try running in Service Mode (recommended), or add HookChildProcesses=0 to your profile. See https://github.com/snowie2000/mactype/wiki/HookChildProcesses for an explanation

- Office 2013 does not use DirectWrite or GDI (it uses its own custom rendering), so Office 2013 doesn't work with MacType. If this bothers you you can use Office 2010 which uses GDI or Office 2016 which uses DirectWrite.

How to build
-------------

Check how to build [document](https://github.com/snowie2000/mactype/blob/master/doc/HOWTOBUILD.md)

