MacType
========================

Better font rendering for Windows.

Latest beta
------------------

[2019.1-beta6](https://github.com/snowie2000/mactype/releases) (Recommended)

Official site
------------------

MacType official site (download is an older release version): 

http://www.mactype.net (Not recommended)

What's new?
------------------

- Win10 compatible
- Updated FreeType (up to git commit 0c4feb72cf976f63d4bf62436bc48f190d0e0c28)
- Support for color fonts :sunglasses:
- New installer
- Lots of bug fixes
- Updates for multi-monitor support
- Tray app can intercept explorer in Service Mode now
- Tweaks for diacritics
- Updates to EasyHook
- Lower CPU in Tray Mode
- Better DirectWrite support thanks to しらいと[http://silight.hatenablog.jp]
- Separate DirectWrite parameter adjustment
- Traditional Chinese localization greatly improved thanks to GT Wang
- English localization improved
- Added Korea localization, thanks to 조현희
- MultiLang system improved
- (Does not include Infinality as this is still experimental)

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

- Office 2013 does not use DirectWrite or GDI (it uses its own custom rendering), so Office 2013 doesn't work with MacType. If this bothers you you can use Office 2010 which uses GDI or Office 2016+ which uses DirectWrite.

- WPS has a builtin defense that **UNLOADS** MacType automatically which can't be turned off. Please contact its software support for solution. We won't to anything to walkaround it.

How to build
-------------

Check how to build [document](https://github.com/snowie2000/mactype/blob/master/doc/HOWTOBUILD.md)

