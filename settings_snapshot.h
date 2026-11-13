#pragma once

#include <memory>
#include <set>
#include <string>

namespace settings_snapshot {

struct AdmissionLists final
{
    std::set<std::wstring> excludeModules;
    std::set<std::wstring> includeModules;
    std::set<std::wstring> unloadModules;
    std::set<int> displayAffinity;
    bool runFromGdiExe = false;
    bool useInclude = false;

    template<class DisplayQuery>
    bool AllowsDisplay(DisplayQuery query) const
    {
        if (displayAffinity.empty())
            return true;
        const int id = query();
        return id < 0 || displayAffinity.find(id) != displayAffinity.end();
    }
};

struct FontFilterLists final
{
    std::set<std::wstring> excludeFonts;
    std::set<std::wstring> includeFonts;

    bool IsExcluded(const std::wstring& family) const
    {
        return excludeFonts.find(family) != excludeFonts.end() ||
            (!includeFonts.empty() && includeFonts.find(family) == includeFonts.end());
    }
};

// Reload clears the current policy even if delayed initialization never finishes.
// Prepare every empty owner first so allocation failure publishes none of them.
template<class SubstitutionTable, class FontLinkTable>
void ResetFontLists(
    std::shared_ptr<const FontFilterLists>& filters,
    std::shared_ptr<const SubstitutionTable>& substitutes,
    std::shared_ptr<const FontLinkTable>& links)
{
    const auto previousLinks = std::atomic_load(&links);
    const auto emptyFilters = std::make_shared<const FontFilterLists>();
    const auto emptySubstitutes = std::make_shared<const SubstitutionTable>();
    auto emptyLinks = std::make_shared<FontLinkTable>();
    if (previousLinks)
        emptyLinks->CopyDefaultsFrom(*previousLinks);

    std::atomic_store(&filters, emptyFilters);
    std::atomic_store(&substitutes, emptySubstitutes);
    std::atomic_store(&links, std::shared_ptr<const FontLinkTable>(emptyLinks));
}

inline bool FolderMatches(
    const std::wstring& normalizedDirectory,
    const std::wstring& normalizedFolder,
    bool prefix)
{
    return prefix ? normalizedDirectory.compare(0, normalizedFolder.size(), normalizedFolder) == 0 :
        normalizedDirectory == normalizedFolder;
}

template<class DirectoryQuery, class Lowercase, class ModuleQuery>
bool MatchesProcess(
    const std::set<std::wstring>& modules,
    DirectoryQuery directory,
    bool prefix,
    Lowercase lowercase,
    ModuleQuery query)
{
    for (const std::wstring& module : modules) {
        if (!module.empty() && module.back() == L'\\') {
            if (FolderMatches(directory(), lowercase(module), prefix))
                return true;
        } else if (query(module.c_str())) {
            return true;
        }
    }
    return false;
}

template<class NameEquals>
bool MatchesExecutable(
    const std::set<std::wstring>& modules,
    const wchar_t* executable,
    NameEquals equal)
{
    for (const std::wstring& module : modules) {
        if (equal(executable, module.c_str()))
            return true;
    }
    return false;
}

} // namespace settings_snapshot
