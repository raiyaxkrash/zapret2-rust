; Перед установкой/обновлением и удалением останавливаем winws2,
; чтобы драйвер WinDivert отпустил файлы.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM winws2.exe'
!macroend

; После установки просим Windows обновить значки: иначе ярлыки и закреплённый
; на панели задач значок показывают старую иконку из кэша.
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'
  nsExec::Exec 'ie4uinit.exe -show'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM winws2.exe'
  nsExec::Exec 'schtasks /Delete /TN "Zapret2 GUI" /F'
!macroend
