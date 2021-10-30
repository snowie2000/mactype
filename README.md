MacType
========================

Better font rendering for Windows.

Latest beta
------------------

[2021.1-RC1](https://github.com/snowie2000/mactype/releases) (Recommended)

Official site
------------------

MacType official site: 

http://www.mactype.net

What's new?
------------------

- Win10 compatible
- CET compatible
- Updated FreeType
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

- WPS has a built in defense that **UNLOADS** MacType automatically which can't be turned off. Please contact its software support for solution. We won't do anything about it.

How to get regitry mode back
-------------

It is no longer possible to enable registry mode via the wizard in Windows 10. 

We have a detailed guide on how you can enable the registry mode manually in [wiki](https://github.com/snowie2000/mactype/wiki/Enable-registry-mode-manually), get your screwdrivers ready before you head over to it.

How to build
-------------

Check how to build [document](https://github.com/snowie2000/mactype/blob/master/doc/HOWTOBUILD.md)

