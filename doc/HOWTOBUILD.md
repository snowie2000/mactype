# How to build

 1. **Compiler / IDE**

    Visual Studio 2019 with v142 toolkit has been tested and is working. Toolkits down to v120 should be able to compile the code, but be aware that the `_xp` ones might refuse to use the Windows 10 SDK.

 2. **Dependencies**

    Mactype depends on
     - [Freetype](https://www.freetype.org/download.html)
       - For the lastest version of Mactype, a customized version of FreeType is required, which can be obtained from https://github.com/snowie2000/freetype    
     - [EasyHook](http://easyhook.github.io/)
     - or Detours (obsolete, better not use)
     - [IniParser (fork)](https://github.com/snowie2000/IniParser)
     - [wow64ext (fork)](https://github.com/snowie2000/rewolf-wow64ext)
     - Windows SDK (10.0.14393.0 or later)

 3. **Building dependencies**

    - FreeType

        Apply `glyph_to_bitmapex.diff` before building.

        Always build multi-thread release.

        Remember to enable options you want in ftoptions.h

        Compile freetype as freetype.lib for x86 and freetype64.lib for x64

        Static library is preferred, you are free to build freetype as independent dlls with better interchangeability but you will lose some compatibility in return, for some programs are delivered with their own copies of freetype which will conflict with your file.

        Set `FREETYPE_PATH` environment variable to root of freetype source.

    - iniParser

        Build as iniparser.lib and iniparser64.lib. Set `INI_PARSER_PATH` environment variable to root of IniParser project.

    - wow64ext

        Build as wow64ext.lib. x64 library is not required. Shared library also works if you prefer that.

    - EasyHook

        Only EasyHookDll project is required.

        Build it as easyhook32.lib and easyhook64.lib, or get the binary distributions.

        Dll filename is not important but you'd better give it a special name to avoid dll confliction as stated above. Do not forget to modify filename in `hook.cpp` of MacType.

    - Windows SDK

        Actually it's not something you need to build, but the installation is tricky.

        One word to rule them all: download **ALL COMPONENTS**  in the installation list! Unless you want to waste several hours looking for these mysterious dependencies it pops to you. Don't worry, you will have a second chance to choose which component you want to install after download.

 4. **Build**

    Last but easiest step: Put all `.lib` files you built earlier into a `lib` folder in the root of MacType, click build and enjoy.

## FAQ

Q: Where are the sources of loader and tuner in the repo?

A: I'm sorry, but they are still closed-source right now. Since you have the mactype source and will surely have a good understanding of how mactype works, I believe it's not a big challenge to write a loader for it.
If you wrote a great loader or something else wonderful, please post an issue or a pull request. Hope we can make MacType better!
