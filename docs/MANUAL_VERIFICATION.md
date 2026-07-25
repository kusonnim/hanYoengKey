# Phase 5 manual verification

Build and launch HanYeongKey, then repeat the following checks in Windows
Notepad, a browser text field, and VS Code.

1. Type `dkssudgktpdy`, select it, and press the Hangul/English key. Confirm it
   becomes `안녕하세요`.
2. Type `ㅗ디ㅣㅐ`, select it, and press the key. Confirm it becomes `hello`.
3. Type `안녕 hello 테스트`, select the whole line, and press the key. Confirm
   it becomes `dkssud hello xptmxm`.
4. Select that result and press the key again. Confirm it becomes
   `안녕 ㅗ디ㅣㅐ 테스트`.
5. Clear the selection and press the key. Confirm the normal Windows input
   language toggle still occurs.
6. Before a conversion, copy rich content (for example formatted browser text
   or an image). After the conversion, paste into an appropriate application
   and confirm all original clipboard formats are still available.
7. Perform several conversions in quick succession and confirm the target
   application and HanYeongKey remain responsive.

The correct two-set key representation of `테스트` is `xptmxm`; the shortened
`xptmx` shown in the original phase brief omits the final `ㅡ` key.

## Known compatibility boundary

Generic Windows UI Automation exposes TextPattern selection ranges as
read-only. HanYeongKey uses ValuePattern only when the selection is verified to
cover the control's entire value; partial selections use the
clipboard-preserving paste fallback so unrelated text cannot be modified.
Elevated applications may reject injected copy/paste input from a non-elevated
HanYeongKey process.
