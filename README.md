MacType
========================

Important notice!
------------------

The following will prevent several unexpected problems:

- For Windows 10 latest version:
  - Until next update **DO NOT** use Registry Mode as it will very likely to crash and lock you out in the logon screen
  - Instead use Service Mode, go to Services -> Mactype Service and change the Startup Type **Automatic (Delayed Start)**
  - Or use Tray mode
- There are some known problems with the new installer as it is a big update, it is very much recommended to:
  - **Stop old MacType, uninstall and reboot**
  - Do a clean install, make sure you chose **Typical install** (not Custom)
- Secure Boot
  - Use **Service Mode** if you prefer to keep Secure Boot enabled (recommended)
  - For **Registry Mode** to work, Secure Boot must be disabled
  - Service Mode and Registry Mode give the same results in most cases so most people will want to chose Service Mode

Latest release version
------------------

1.2017.628.0

Binary Installer
------------------

Visit MacType official site to download: 

http://www.mactype.net

What's new?
------------------

- Win10 compatible
- Traditional Chinese localization has been greatly improved thanks to GT Wang.
- MultiLang system improved.
- Better DirectWrite support thanks to しらいと[http://silight.hatenablog.jp].
- FreeType 2.8.0 included.
- Two-stage mactype loader introduced.
- Separate DirectWrite parameter adjustment.
- ClipboxFix is reverted to 0 by default to avoid some incompatibility issues.
- Added Korea localization, thanks to 조현희

Donation
------------------

MacType now accepts donations. 

Please visit http://www.mactype.net and keep an eye on the bottom right corner :heart:

Thank you for your support! Your donations will keep the server running, keep me updating, and buy more coffees :coffee:

Known issues
---------------

- Please backup your profile before upgrading!

- Only Chinese simplified/Traditional and English are fully localized, some options may missing in MacType Tuner due to the strings missing in the language file.

- If you want to use MacType-patch together with MacType official release, Do remember to add DirectWrite=0 to your profile or you will have mysterious problems.


How to build
-------------

Check how to build [document](https://github.com/snowie2000/mactype/blob/master/doc/HOWTOBUILD.md)

