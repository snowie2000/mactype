How to build
-------------

Personally, I use Visual Studio 2013 (msvc) to build mactype and all its dependencies, however, I believe you can build them with other compiles.
Here I will show you the steps I do to make the compilation.

 1. **Compiler / IDE**
	
    msvc is preferred, I have provided a solution of msvc2013 in the repo. If you are using msvc2013+, just open my solution and you are ready to go. For msvc version lower than 2013, you have to make your own project.
	
 2. **Dependencies**
	 
    Mactype depends on
	 - Freetype ([link](https://www.freetype.org/download.html))
	 - EasyHook ([link](http://easyhook.github.io/))
	 - or Detours(obsolete, better not use)
	 - Windows sdk 10.0.14393.0 or later([link](https://developer.microsoft.com/en-us/windows/downloads/windows-10-sdk))

3. **Building dependencies**
	- FreeType
		
        Always build multi-thread release.

		Remember to enable options you want in ftoptions.h
	
		Compile freetype as Freetype.lib for x86 and freetype64.lib for x64
	
		Static library is preferred, you are free to build freetype as independent dlls with better interchangeability but you will lose some compatibility in return, for some programs are delivered with their own copies of freetype which will conflict with your file.
	
	- EasyHook
		
        Only EasyHookDll project is required.

		Build it as easyhk32.lib and easyhk64.lib.
	
		Dll filename is not important but you'd better give it a special name to avoid dll confliction as I stated above.
	- Windows SDK
		
        Actually it's not something you need to build, but the installation is tricky.

		One word to rule them all: download **ALL COMPONENTS**  in the installation list! Unless you want to waste several hours looking for these mysterious dependencies it pops to you. Don't worry, you will have a second chance to choose which component you want to install after download.
		
4. **Build**

	Last but simplest step. Put all files you builds in the above steps to MacType folder, set up VC++ folders and hit F7.
	Enjoy. 

FAQ
-------
Q: Where are the sources of loader and tunner in the repo?

A: I'm sorry, but they are still close-source right now. Since you have the mactype source and will surely have a good understanding of how mactype works, I believe it's not a big challenge to write a loader for it.
If you wrote a great loader or something else wonderful, don't forget to send me link~
