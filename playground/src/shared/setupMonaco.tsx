/**
 * Non-rendering component that loads the Monaco editor themes.
 */

import { Monaco } from "@monaco-editor/react";

export const WHITE = "#ffffff";
export const RADIATE = "#d7ff64";
export const FLARE = "#6340ac";
export const ROCK = "#78876e";
export const GALAXY = "#261230";
export const SPACE = "#30173d";
export const COMET = "#6f5d6f";
export const COSMIC = "#de5fe9";
export const SUN = "#ffac2f";
export const ELECTRON = "#46ebe1";
export const AURORA = "#46eb74";
export const CONSTELLATION = "#5f6de9";
export const NEUTRON = "#cff3cf";
export const PROTON = "#f6afbc";
export const NEBULA = "#cdcbfb";
export const SUPERNOVA = "#f1aff6";
export const STARLIGHT = "#f4f4f1";
export const LUNAR = "#fbf2fc";
export const ASTEROID = "#e3cee3";
export const CRATER = "#f0dfdf";

export function setupMonaco(monaco: Monaco) {
  defineAyuThemes(monaco);
  defineFirLanguage(monaco);
  defineRustPythonTokensLanguage(monaco);
  defineRustPythonAstLanguage(monaco);
  defineCommentsLanguage(monaco);
}

function defineAyuThemes(monaco: Monaco) {
  // Generated via `monaco-vscode-textmate-theme-converter`.
  // See: https://github.com/ayu-theme/vscode-ayu/blob/91839e8a9dfa78d61e58dbcf9b52272a01fee66a/ayu-light.json.
  monaco.editor.defineTheme("Ayu-Light", {
    inherit: false,
    base: "vs",
    colors: {
      focusBorder: "#ffaa33b3",
      foreground: "#8a9199",
      "widget.shadow": "#00000026",
      "selection.background": "#035bd626",
      "icon.foreground": "#8a9199",
      errorForeground: "#e65050",
      descriptionForeground: "#8a9199",
      "textBlockQuote.background": "#f3f4f5",
      "textLink.foreground": "#ffaa33",
      "textLink.activeForeground": "#ffaa33",
      "textPreformat.foreground": "#5c6166",
      "button.background": "#ffaa33",
      "button.foreground": "#f8f9fa",
      "button.hoverBackground": "#f9a52e",
      "button.secondaryBackground": "#8a919933",
      "button.secondaryForeground": "#5c6166",
      "button.secondaryHoverBackground": "#8a919980",
      "dropdown.background": "#fcfcfc",
      "dropdown.foreground": "#8a9199",
      "dropdown.border": "#8a919945",
      "input.background": "#fcfcfc",
      "input.border": "#8a919945",
      "input.foreground": "#5c6166",
      "input.placeholderForeground": "#8a919980",
      "inputOption.activeBorder": "#f4a0284d",
      "inputOption.activeBackground": "#ffaa3333",
      "inputOption.activeForeground": "#f4a028",
      "inputValidation.errorBackground": "#fcfcfc",
      "inputValidation.errorBorder": "#e65050",
      "inputValidation.infoBackground": "#f8f9fa",
      "inputValidation.infoBorder": "#55b4d4",
      "inputValidation.warningBackground": "#f8f9fa",
      "inputValidation.warningBorder": "#f2ae49",
      "scrollbar.shadow": "#6b7d8f00",
      "scrollbarSlider.background": "#8a919966",
      "scrollbarSlider.hoverBackground": "#8a919999",
      "scrollbarSlider.activeBackground": "#8a9199b3",
      "badge.background": "#ffaa3333",
      "badge.foreground": "#f4a028",
      "progressBar.background": "#ffaa33",
      "list.activeSelectionBackground": "#56728f1f",
      "list.activeSelectionForeground": "#5c6166",
      "list.focusBackground": "#56728f1f",
      "list.focusForeground": "#5c6166",
      "list.focusOutline": "#56728f1f",
      "list.highlightForeground": "#ffaa33",
      "list.deemphasizedForeground": "#e65050",
      "list.hoverBackground": "#56728f1f",
      "list.inactiveSelectionBackground": "#6b7d8f1f",
      "list.inactiveSelectionForeground": "#8a9199",
      "list.invalidItemForeground": "#8a91994d",
      "list.errorForeground": "#e65050",
      "tree.indentGuidesStroke": "#8a919959",
      "listFilterWidget.background": "#f3f4f5",
      "listFilterWidget.outline": "#ffaa33",
      "listFilterWidget.noMatchesOutline": "#e65050",
      "list.filterMatchBackground": "#8f30efcc",
      "list.filterMatchBorder": "#9f40ffcc",
      "activityBar.background": "#f8f9fa",
      "activityBar.foreground": "#8a9199cc",
      "activityBar.inactiveForeground": "#8a919999",
      "activityBar.border": "#f8f9fa",
      "activityBar.activeBorder": "#ffaa33b3",
      "activityBarBadge.background": "#ffaa33",
      "activityBarBadge.foreground": "#f8f9fa",
      "sideBar.background": "#f8f9fa",
      "sideBar.border": "#f8f9fa",
      "sideBarTitle.foreground": "#8a9199",
      "sideBarSectionHeader.background": "#f8f9fa",
      "sideBarSectionHeader.foreground": "#8a9199",
      "sideBarSectionHeader.border": "#f8f9fa",
      "minimap.background": "#f8f9fa",
      "minimap.selectionHighlight": "#035bd626",
      "minimap.errorHighlight": "#e65050",
      "minimap.findMatchHighlight": "#9f40ff2b",
      "minimapGutter.addedBackground": "#6cbf43",
      "minimapGutter.modifiedBackground": "#478acc",
      "minimapGutter.deletedBackground": "#ff7383",
      "editorGroup.border": "#6b7d8f1f",
      "editorGroup.background": "#f3f4f5",
      "editorGroupHeader.noTabsBackground": "#f8f9fa",
      "editorGroupHeader.tabsBackground": "#f8f9fa",
      "editorGroupHeader.tabsBorder": "#f8f9fa",
      "tab.activeBackground": "#f8f9fa",
      "tab.activeForeground": "#5c6166",
      "tab.border": "#f8f9fa",
      "tab.activeBorder": "#ffaa33",
      "tab.unfocusedActiveBorder": "#8a9199",
      "tab.inactiveBackground": "#f8f9fa",
      "tab.inactiveForeground": "#8a9199",
      "tab.unfocusedActiveForeground": "#8a9199",
      "tab.unfocusedInactiveForeground": "#8a9199",
      "editor.background": "#f8f9fa",
      "editor.foreground": "#5c6166",
      "editorLineNumber.foreground": "#8a919966",
      "editorLineNumber.activeForeground": "#8a9199cc",
      "editorCursor.foreground": "#ffaa33",
      "editor.inactiveSelectionBackground": "#035bd612",
      "editor.selectionBackground": "#035bd626",
      "editor.selectionHighlightBackground": "#6cbf4326",
      "editor.selectionHighlightBorder": "#6cbf4300",
      "editor.wordHighlightBackground": "#478acc14",
      "editor.wordHighlightStrongBackground": "#6cbf4314",
      "editor.wordHighlightBorder": "#478acc80",
      "editor.wordHighlightStrongBorder": "#6cbf4380",
      "editor.findMatchBackground": "#9f40ff2b",
      "editor.findMatchBorder": "#9f40ff2b",
      "editor.findMatchHighlightBackground": "#9f40ffcc",
      "editor.findMatchHighlightBorder": "#8f30efcc",
      "editor.findRangeHighlightBackground": "#9f40ff40",
      "editor.rangeHighlightBackground": "#9f40ff33",
      "editor.lineHighlightBackground": "#8a91991a",
      "editorLink.activeForeground": "#ffaa33",
      "editorWhitespace.foreground": "#8a919966",
      "editorIndentGuide.background": "#8a91992e",
      "editorIndentGuide.activeBackground": "#8a919959",
      "editorRuler.foreground": "#8a91992e",
      "editorCodeLens.foreground": "#787b8099",
      "editorBracketMatch.background": "#8a91994d",
      "editorBracketMatch.border": "#8a91994d",
      "editor.snippetTabstopHighlightBackground": "#6cbf4333",
      "editorOverviewRuler.border": "#6b7d8f1f",
      "editorOverviewRuler.modifiedForeground": "#478acc",
      "editorOverviewRuler.addedForeground": "#6cbf43",
      "editorOverviewRuler.deletedForeground": "#ff7383",
      "editorOverviewRuler.errorForeground": "#e65050",
      "editorOverviewRuler.warningForeground": "#ffaa33",
      "editorOverviewRuler.bracketMatchForeground": "#8a9199b3",
      "editorOverviewRuler.wordHighlightForeground": "#478acc66",
      "editorOverviewRuler.wordHighlightStrongForeground": "#6cbf4366",
      "editorOverviewRuler.findMatchForeground": "#9f40ff2b",
      "editorError.foreground": "#e65050",
      "editorWarning.foreground": "#ffaa33",
      "editorGutter.modifiedBackground": "#478acccc",
      "editorGutter.addedBackground": "#6cbf43cc",
      "editorGutter.deletedBackground": "#ff7383cc",
      "diffEditor.insertedTextBackground": "#6cbf431f",
      "diffEditor.removedTextBackground": "#ff73831f",
      "diffEditor.diagonalFill": "#6b7d8f1f",
      "editorWidget.background": "#f3f4f5",
      "editorWidget.border": "#6b7d8f1f",
      "editorHoverWidget.background": "#f3f4f5",
      "editorHoverWidget.border": "#6b7d8f1f",
      "editorSuggestWidget.background": "#f3f4f5",
      "editorSuggestWidget.border": "#6b7d8f1f",
      "editorSuggestWidget.highlightForeground": "#ffaa33",
      "editorSuggestWidget.selectedBackground": "#56728f1f",
      "debugExceptionWidget.border": "#6b7d8f1f",
      "debugExceptionWidget.background": "#f3f4f5",
      "editorMarkerNavigation.background": "#f3f4f5",
      "peekView.border": "#56728f1f",
      "peekViewTitle.background": "#56728f1f",
      "peekViewTitleDescription.foreground": "#8a9199",
      "peekViewTitleLabel.foreground": "#5c6166",
      "peekViewEditor.background": "#f3f4f5",
      "peekViewEditor.matchHighlightBackground": "#9f40ffcc",
      "peekViewEditor.matchHighlightBorder": "#8f30efcc",
      "peekViewResult.background": "#f3f4f5",
      "peekViewResult.fileForeground": "#5c6166",
      "peekViewResult.lineForeground": "#8a9199",
      "peekViewResult.matchHighlightBackground": "#9f40ffcc",
      "peekViewResult.selectionBackground": "#56728f1f",
      "panel.background": "#f8f9fa",
      "panel.border": "#6b7d8f1f",
      "panelTitle.activeBorder": "#ffaa33",
      "panelTitle.activeForeground": "#5c6166",
      "panelTitle.inactiveForeground": "#8a9199",
      "statusBar.background": "#f8f9fa",
      "statusBar.foreground": "#8a9199",
      "statusBar.border": "#f8f9fa",
      "statusBar.debuggingBackground": "#ed9366",
      "statusBar.debuggingForeground": "#fcfcfc",
      "statusBar.noFolderBackground": "#f3f4f5",
      "statusBarItem.activeBackground": "#8a919933",
      "statusBarItem.hoverBackground": "#8a919933",
      "statusBarItem.prominentBackground": "#6b7d8f1f",
      "statusBarItem.prominentHoverBackground": "#00000030",
      "statusBarItem.remoteBackground": "#ffaa33",
      "statusBarItem.remoteForeground": "#fcfcfc",
      "titleBar.activeBackground": "#f8f9fa",
      "titleBar.activeForeground": "#5c6166",
      "titleBar.inactiveBackground": "#f8f9fa",
      "titleBar.inactiveForeground": "#8a9199",
      "titleBar.border": "#f8f9fa",
      "extensionButton.prominentForeground": "#fcfcfc",
      "extensionButton.prominentBackground": "#ffaa33",
      "extensionButton.prominentHoverBackground": "#f9a52e",
      "pickerGroup.border": "#6b7d8f1f",
      "pickerGroup.foreground": "#8a919980",
      "debugToolBar.background": "#f3f4f5",
      "debugIcon.breakpointForeground": "#ed9366",
      "debugIcon.breakpointDisabledForeground": "#ed936680",
      "debugConsoleInputIcon.foreground": "#ffaa33",
      "welcomePage.tileBackground": "#f8f9fa",
      "welcomePage.tileShadow": "#00000026",
      "welcomePage.progress.background": "#8a91991a",
      "welcomePage.buttonBackground": "#ffaa3366",
      "walkThrough.embeddedEditorBackground": "#f3f4f5",
      "gitDecoration.modifiedResourceForeground": "#478accb3",
      "gitDecoration.deletedResourceForeground": "#ff7383b3",
      "gitDecoration.untrackedResourceForeground": "#6cbf43b3",
      "gitDecoration.ignoredResourceForeground": "#8a919980",
      "gitDecoration.conflictingResourceForeground": "",
      "gitDecoration.submoduleResourceForeground": "#a37accb3",
      "settings.headerForeground": "#5c6166",
      "settings.modifiedItemIndicator": "#478acc",
      "keybindingLabel.background": "#8a91991a",
      "keybindingLabel.foreground": "#5c6166",
      "keybindingLabel.border": "#5c61661a",
      "keybindingLabel.bottomBorder": "#5c61661a",
      "terminal.background": "#f8f9fa",
      "terminal.foreground": "#5c6166",
      "terminal.ansiBlack": "#000000",
      "terminal.ansiRed": "#ea6c6d",
      "terminal.ansiGreen": "#6cbf43",
      "terminal.ansiYellow": "#eca944",
      "terminal.ansiBlue": "#3199e1",
      "terminal.ansiMagenta": "#9e75c7",
      "terminal.ansiCyan": "#46ba94",
      "terminal.ansiWhite": "#c7c7c7",
      "terminal.ansiBrightBlack": "#686868",
      "terminal.ansiBrightRed": "#f07171",
      "terminal.ansiBrightGreen": "#86b300",
      "terminal.ansiBrightYellow": "#f2ae49",
      "terminal.ansiBrightBlue": "#399ee6",
      "terminal.ansiBrightMagenta": "#a37acc",
      "terminal.ansiBrightCyan": "#4cbf99",
      "terminal.ansiBrightWhite": "#d1d1d1",
    },
    rules: [
      {
        fontStyle: "italic",
        foreground: "#787b8099",
        token: "comment",
      },
      {
        foreground: ROCK,
        token: "string",
      },
      {
        foreground: SUN,
        token: "keyword",
      },
      {
        foreground: CONSTELLATION,
        token: "number",
      },
      {
        token: "tag",
        foreground: ROCK,
      },
    ],
    encodedTokensColors: [],
  });

  // Generated via `monaco-vscode-textmate-theme-converter`.
  // See: https://github.com/ayu-theme/vscode-ayu/blob/91839e8a9dfa78d61e58dbcf9b52272a01fee66a/ayu-dark.json.
  monaco.editor.defineTheme("Ayu-Dark", {
    inherit: false,
    base: "vs-dark",
    colors: {
      focusBorder: "#e6b450b3",
      foreground: "#565b66",
      "widget.shadow": "#00000080",
      "selection.background": "#409fff4d",
      "icon.foreground": "#565b66",
      errorForeground: "#d95757",
      descriptionForeground: "#565b66",
      "textBlockQuote.background": "#0f131a",
      "textLink.foreground": "#e6b450",
      "textLink.activeForeground": "#e6b450",
      "textPreformat.foreground": "#bfbdb6",
      "button.background": "#e6b450",
      "button.foreground": "#0b0e14",
      "button.hoverBackground": "#e1af4b",
      "button.secondaryBackground": "#565b6633",
      "button.secondaryForeground": "#bfbdb6",
      "button.secondaryHoverBackground": "#565b6680",
      "dropdown.background": "#0d1017",
      "dropdown.foreground": "#565b66",
      "dropdown.border": "#565b6645",
      "input.background": "#0d1017",
      "input.border": "#565b6645",
      "input.foreground": "#bfbdb6",
      "input.placeholderForeground": "#565b6680",
      "inputOption.activeBorder": "#e6b4504d",
      "inputOption.activeBackground": "#e6b45033",
      "inputOption.activeForeground": "#e6b450",
      "inputValidation.errorBackground": "#0d1017",
      "inputValidation.errorBorder": "#d95757",
      "inputValidation.infoBackground": "#0b0e14",
      "inputValidation.infoBorder": "#39bae6",
      "inputValidation.warningBackground": "#0b0e14",
      "inputValidation.warningBorder": "#ffb454",
      "scrollbar.shadow": "#11151c00",
      "scrollbarSlider.background": "#565b6666",
      "scrollbarSlider.hoverBackground": "#565b6699",
      "scrollbarSlider.activeBackground": "#565b66b3",
      "badge.background": "#e6b45033",
      "badge.foreground": "#e6b450",
      "progressBar.background": "#e6b450",
      "list.activeSelectionBackground": "#47526640",
      "list.activeSelectionForeground": "#bfbdb6",
      "list.focusBackground": "#47526640",
      "list.focusForeground": "#bfbdb6",
      "list.focusOutline": "#47526640",
      "list.highlightForeground": "#e6b450",
      "list.deemphasizedForeground": "#d95757",
      "list.hoverBackground": "#47526640",
      "list.inactiveSelectionBackground": "#47526633",
      "list.inactiveSelectionForeground": "#565b66",
      "list.invalidItemForeground": "#565b664d",
      "list.errorForeground": "#d95757",
      "tree.indentGuidesStroke": "#6c738080",
      "listFilterWidget.background": "#0f131a",
      "listFilterWidget.outline": "#e6b450",
      "listFilterWidget.noMatchesOutline": "#d95757",
      "list.filterMatchBackground": "#5f4c7266",
      "list.filterMatchBorder": "#6c598066",
      "activityBar.background": "#0b0e14",
      "activityBar.foreground": "#565b66cc",
      "activityBar.inactiveForeground": "#565b6699",
      "activityBar.border": "#0b0e14",
      "activityBar.activeBorder": "#e6b450b3",
      "activityBarBadge.background": "#e6b450",
      "activityBarBadge.foreground": "#0b0e14",
      "sideBar.background": "#0b0e14",
      "sideBar.border": "#0b0e14",
      "sideBarTitle.foreground": "#565b66",
      "sideBarSectionHeader.background": "#0b0e14",
      "sideBarSectionHeader.foreground": "#565b66",
      "sideBarSectionHeader.border": "#0b0e14",
      "minimap.background": "#0b0e14",
      "minimap.selectionHighlight": "#409fff4d",
      "minimap.errorHighlight": "#d95757",
      "minimap.findMatchHighlight": "#6c5980",
      "minimapGutter.addedBackground": "#7fd962",
      "minimapGutter.modifiedBackground": "#73b8ff",
      "minimapGutter.deletedBackground": "#f26d78",
      "editorGroup.border": "#11151c",
      "editorGroup.background": "#0f131a",
      "editorGroupHeader.noTabsBackground": "#0b0e14",
      "editorGroupHeader.tabsBackground": "#0b0e14",
      "editorGroupHeader.tabsBorder": "#0b0e14",
      "tab.activeBackground": "#0b0e14",
      "tab.activeForeground": "#bfbdb6",
      "tab.border": "#0b0e14",
      "tab.activeBorder": "#e6b450",
      "tab.unfocusedActiveBorder": "#565b66",
      "tab.inactiveBackground": "#0b0e14",
      "tab.inactiveForeground": "#565b66",
      "tab.unfocusedActiveForeground": "#565b66",
      "tab.unfocusedInactiveForeground": "#565b66",
      "editor.background": "#0b0e14",
      "editor.foreground": "#bfbdb6",
      "editorLineNumber.foreground": "#6c738099",
      "editorLineNumber.activeForeground": "#6c7380e6",
      "editorCursor.foreground": "#e6b450",
      "editor.inactiveSelectionBackground": "#409fff21",
      "editor.selectionBackground": "#409fff4d",
      "editor.selectionHighlightBackground": "#7fd96226",
      "editor.selectionHighlightBorder": "#7fd96200",
      "editor.wordHighlightBackground": "#73b8ff14",
      "editor.wordHighlightStrongBackground": "#7fd96214",
      "editor.wordHighlightBorder": "#73b8ff80",
      "editor.wordHighlightStrongBorder": "#7fd96280",
      "editor.findMatchBackground": "#6c5980",
      "editor.findMatchBorder": "#6c5980",
      "editor.findMatchHighlightBackground": "#6c598066",
      "editor.findMatchHighlightBorder": "#5f4c7266",
      "editor.findRangeHighlightBackground": "#6c598040",
      "editor.rangeHighlightBackground": "#6c598033",
      "editor.lineHighlightBackground": "#131721",
      "editorLink.activeForeground": "#e6b450",
      "editorWhitespace.foreground": "#6c738099",
      "editorIndentGuide.background": "#6c738033",
      "editorIndentGuide.activeBackground": "#6c738080",
      "editorRuler.foreground": "#6c738033",
      "editorCodeLens.foreground": "#acb6bf8c",
      "editorBracketMatch.background": "#6c73804d",
      "editorBracketMatch.border": "#6c73804d",
      "editor.snippetTabstopHighlightBackground": "#7fd96233",
      "editorOverviewRuler.border": "#11151c",
      "editorOverviewRuler.modifiedForeground": "#73b8ff",
      "editorOverviewRuler.addedForeground": "#7fd962",
      "editorOverviewRuler.deletedForeground": "#f26d78",
      "editorOverviewRuler.errorForeground": "#d95757",
      "editorOverviewRuler.warningForeground": "#e6b450",
      "editorOverviewRuler.bracketMatchForeground": "#6c7380b3",
      "editorOverviewRuler.wordHighlightForeground": "#73b8ff66",
      "editorOverviewRuler.wordHighlightStrongForeground": "#7fd96266",
      "editorOverviewRuler.findMatchForeground": "#6c5980",
      "editorError.foreground": "#d95757",
      "editorWarning.foreground": "#e6b450",
      "editorGutter.modifiedBackground": "#73b8ffcc",
      "editorGutter.addedBackground": "#7fd962cc",
      "editorGutter.deletedBackground": "#f26d78cc",
      "diffEditor.insertedTextBackground": "#7fd9621f",
      "diffEditor.removedTextBackground": "#f26d781f",
      "diffEditor.diagonalFill": "#11151c",
      "editorWidget.background": "#0f131a",
      "editorWidget.border": "#11151c",
      "editorHoverWidget.background": "#0f131a",
      "editorHoverWidget.border": "#11151c",
      "editorSuggestWidget.background": "#0f131a",
      "editorSuggestWidget.border": "#11151c",
      "editorSuggestWidget.highlightForeground": "#e6b450",
      "editorSuggestWidget.selectedBackground": "#47526640",
      "debugExceptionWidget.border": "#11151c",
      "debugExceptionWidget.background": "#0f131a",
      "editorMarkerNavigation.background": "#0f131a",
      "peekView.border": "#47526640",
      "peekViewTitle.background": "#47526640",
      "peekViewTitleDescription.foreground": "#565b66",
      "peekViewTitleLabel.foreground": "#bfbdb6",
      "peekViewEditor.background": "#0f131a",
      "peekViewEditor.matchHighlightBackground": "#6c598066",
      "peekViewEditor.matchHighlightBorder": "#5f4c7266",
      "peekViewResult.background": "#0f131a",
      "peekViewResult.fileForeground": "#bfbdb6",
      "peekViewResult.lineForeground": "#565b66",
      "peekViewResult.matchHighlightBackground": "#6c598066",
      "peekViewResult.selectionBackground": "#47526640",
      "panel.background": "#0b0e14",
      "panel.border": "#11151c",
      "panelTitle.activeBorder": "#e6b450",
      "panelTitle.activeForeground": "#bfbdb6",
      "panelTitle.inactiveForeground": "#565b66",
      "statusBar.background": "#0b0e14",
      "statusBar.foreground": "#565b66",
      "statusBar.border": "#0b0e14",
      "statusBar.debuggingBackground": "#f29668",
      "statusBar.debuggingForeground": "#0d1017",
      "statusBar.noFolderBackground": "#0f131a",
      "statusBarItem.activeBackground": "#565b6633",
      "statusBarItem.hoverBackground": "#565b6633",
      "statusBarItem.prominentBackground": "#11151c",
      "statusBarItem.prominentHoverBackground": "#00000030",
      "statusBarItem.remoteBackground": "#e6b450",
      "statusBarItem.remoteForeground": "#0d1017",
      "titleBar.activeBackground": "#0b0e14",
      "titleBar.activeForeground": "#bfbdb6",
      "titleBar.inactiveBackground": "#0b0e14",
      "titleBar.inactiveForeground": "#565b66",
      "titleBar.border": "#0b0e14",
      "extensionButton.prominentForeground": "#0d1017",
      "extensionButton.prominentBackground": "#e6b450",
      "extensionButton.prominentHoverBackground": "#e1af4b",
      "pickerGroup.border": "#11151c",
      "pickerGroup.foreground": "#565b6680",
      "debugToolBar.background": "#0f131a",
      "debugIcon.breakpointForeground": "#f29668",
      "debugIcon.breakpointDisabledForeground": "#f2966880",
      "debugConsoleInputIcon.foreground": "#e6b450",
      "welcomePage.tileBackground": "#0b0e14",
      "welcomePage.tileShadow": "#00000080",
      "welcomePage.progress.background": "#131721",
      "welcomePage.buttonBackground": "#e6b45066",
      "walkThrough.embeddedEditorBackground": "#0f131a",
      "gitDecoration.modifiedResourceForeground": "#73b8ffb3",
      "gitDecoration.deletedResourceForeground": "#f26d78b3",
      "gitDecoration.untrackedResourceForeground": "#7fd962b3",
      "gitDecoration.ignoredResourceForeground": "#565b6680",
      "gitDecoration.conflictingResourceForeground": "",
      "gitDecoration.submoduleResourceForeground": "#d2a6ffb3",
      "settings.headerForeground": "#bfbdb6",
      "settings.modifiedItemIndicator": "#73b8ff",
      "keybindingLabel.background": "#565b661a",
      "keybindingLabel.foreground": "#bfbdb6",
      "keybindingLabel.border": "#bfbdb61a",
      "keybindingLabel.bottomBorder": "#bfbdb61a",
      "terminal.background": "#0b0e14",
      "terminal.foreground": "#bfbdb6",
      "terminal.ansiBlack": "#11151c",
      "terminal.ansiRed": "#ea6c73",
      "terminal.ansiGreen": "#7fd962",
      "terminal.ansiYellow": "#f9af4f",
      "terminal.ansiBlue": "#53bdfa",
      "terminal.ansiMagenta": "#cda1fa",
      "terminal.ansiCyan": "#90e1c6",
      "terminal.ansiWhite": "#c7c7c7",
      "terminal.ansiBrightBlack": "#686868",
      "terminal.ansiBrightRed": "#f07178",
      "terminal.ansiBrightGreen": "#aad94c",
      "terminal.ansiBrightYellow": "#ffb454",
      "terminal.ansiBrightBlue": "#59c2ff",
      "terminal.ansiBrightMagenta": "#d2a6ff",
      "terminal.ansiBrightCyan": "#95e6cb",
      "terminal.ansiBrightWhite": "#ffffff",
    },
    rules: [
      {
        fontStyle: "italic",
        foreground: "#acb6bf8c",
        token: "comment",
      },
      {
        foreground: RADIATE,
        token: "string",
      },
      {
        foreground: ELECTRON,
        token: "number",
      },
      {
        foreground: STARLIGHT,
        token: "identifier",
      },
      {
        foreground: SUN,
        token: "keyword",
      },
      {
        foreground: PROTON,
        token: "tag",
      },
      {
        foreground: ASTEROID,
        token: "delimiter",
      },
    ],
    encodedTokensColors: [],
  });
}

// https://microsoft.github.io/monaco-editor/monarch.html
function defineRustPythonAstLanguage(monaco: Monaco) {
  monaco.languages.register({
    id: "RustPythonAst",
  });

  monaco.languages.setMonarchTokensProvider("RustPythonAst", {
    keywords: ["None", "Err"],
    tokenizer: {
      root: [
        [
          /[a-zA-Z_$][\w$]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@default": "identifier",
            },
          },
        ],

        // Whitespace
        [/[ \t\r\n]+/, "white"],

        // Strings
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],

        [/\d+/, "number"],

        [/[{}()[\]]/, "@brackets"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\[\\"]/, "string.escape"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
    brackets: [
      {
        open: "(",
        close: ")",
        token: "delimiter.parenthesis",
      },
      {
        open: "{",
        close: "}",
        token: "delimiter.curly",
      },
      {
        open: "[",
        close: "]",
        token: "delimiter.bracket",
      },
    ],
  });
}

// Modeled after 'RustPythonAst'
function defineCommentsLanguage(monaco: Monaco) {
  monaco.languages.register({
    id: "Comments",
  });

  monaco.languages.setMonarchTokensProvider("Comments", {
    keywords: ["None", "Err"],
    tokenizer: {
      root: [
        [
          /[a-zA-Z_$][\w$]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@default": "identifier",
            },
          },
        ],

        // Whitespace
        [/[ \t\r\n]+/, "white"],

        // Strings
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],

        [/\d+/, "number"],

        [/[{}()[\]]/, "@brackets"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\[\\"]/, "string.escape"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
    brackets: [
      {
        open: "(",
        close: ")",
        token: "delimiter.parenthesis",
      },
      {
        open: "{",
        close: "}",
        token: "delimiter.curly",
      },
      {
        open: "[",
        close: "]",
        token: "delimiter.bracket",
      },
    ],
  });
}

function defineRustPythonTokensLanguage(monaco: Monaco) {
  monaco.languages.register({
    id: "RustPythonTokens",
  });

  monaco.languages.setMonarchTokensProvider("RustPythonTokens", {
    keywords: ["Ok", "Err"],
    tokenizer: {
      root: [
        [
          /[a-zA-Z_$][\w$]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@default": "identifier",
            },
          },
        ],

        // Whitespace
        [/[ \t\r\n]+/, "white"],

        // Strings
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],

        [/\d+/, "number"],

        [/[{}()[\]]/, "@brackets"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\[\\"]/, "string.escape"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
    brackets: [
      {
        open: "(",
        close: ")",
        token: "delimiter.parenthesis",
      },
      {
        open: "{",
        close: "}",
        token: "delimiter.curly",
      },
      {
        open: "[",
        close: "]",
        token: "delimiter.bracket",
      },
    ],
  });
}

function defineFirLanguage(monaco: Monaco) {
  monaco.languages.register({
    id: "fir",
  });

  monaco.languages.setMonarchTokensProvider("fir", {
    tokenizer: {
      root: [
        [/[a-z_$][\w$]*/, "keyword"],

        // Whitespace
        [/[ \t\r\n]+/, "white"],

        [/[()[\]<>]/, "@brackets"],

        // Strings
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],

        [/\d+/, "number"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\[\\"]/, "string.escape"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
    brackets: [
      {
        open: "(",
        close: ")",
        token: "delimiter.parenthesis",
      },
      {
        open: "[",
        close: "]",
        token: "delimiter.bracket",
      },
      {
        open: "<",
        close: ">",
        token: "delimiter.angle",
      },
    ],
  });
}
