; Перед установкой/обновлением и удалением останавливаем winws2,
; чтобы драйвер WinDivert отпустил файлы.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM winws2.exe'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM winws2.exe'
  nsExec::Exec 'schtasks /Delete /TN "Zapret2 GUI" /F'
!macroend
