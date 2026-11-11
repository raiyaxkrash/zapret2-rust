//! Автозапуск через Планировщик заданий: обычный автозапуск (реестр Run)
//! не умеет запускать программы с правами администратора без запроса UAC.

use std::fs;

use crate::util::hidden_command;

pub const TASK_NAME: &str = "Zapret2 GUI";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn is_enabled() -> bool {
    hidden_command("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn set(enabled: bool) -> Result<(), String> {
    if !enabled {
        if is_enabled() {
            let out = hidden_command("schtasks")
                .args(["/Delete", "/TN", TASK_NAME, "/F"])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err("Не удалось удалить задание автозапуска".into());
            }
        }
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => return Err("Не удалось определить пользователя".into()),
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Запуск Zapret2 GUI при входе в систему</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT5S</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--autostart</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(&user),
        exe = xml_escape(&exe.to_string_lossy()),
    );

    // schtasks надёжно читает XML только в UTF-16 LE с BOM.
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let tmp = std::env::temp_dir().join("zapret2-gui-task.xml");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let out = hidden_command("schtasks")
        .args(["/Create", "/F", "/TN", TASK_NAME, "/XML"])
        .arg(&tmp)
        .output()
        .map_err(|e| e.to_string());
    let _ = fs::remove_file(&tmp);
    let out = out?;
    if !out.status.success() {
        return Err(format!(
            "Не удалось создать задание автозапуска: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}
