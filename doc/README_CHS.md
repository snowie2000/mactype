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

捐赠
------------------

MacType 现在接收捐赠. 

请访问 https://www.mactype.net 然后注意下右下角的咖啡哦 :heart:

感谢您的支持，您的捐赠将用于服务器的运营，并保持我们的开发热情（以及用于买奶茶咖啡） :coffee:

已知问题
---------------

- 升级前请备份您的配置文件，升级将删除您原有的配置!

- 目前仅简体中文、繁体中文和英语实现了完全本地化，其他语言由于本地化的缺失可能在使用MacTuner时会存在选项缺失等问题，我们非常抱歉。我们非常欢迎您能帮助我们提供或优化我们的翻译。

- 如果您要和 MacType-patch 一起使用，请一定记得设置 DirectWrite=0, 否则可能会出现无法预料的问题。

- 如果您正在使用64位的Windows，部分安全软件、杀毒软件可能会和MacType冲突。这些软件会误认为MacType尝试修改运行中的程序。 一个可行的方案是使用“服务模式”，并关闭子进程加载（HookChildProcesses=0）详见 https://github.com/snowie2000/mactype/wiki/HookChildProcesses。请注意，关闭子进程加载后，Chrome和firefox可能无法被渲染，并且UWP程序将无法被渲染。

从源码构建
-------------

请参照文档从源码编译（英文） [构建文档](https://github.com/snowie2000/mactype/blob/master/doc/HOWTOBUILD.md)

