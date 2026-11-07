//! Windows registration adapter. Caller proves zero owners before activate/rollback.
//! Maintenance single-writer scope: 기준_release032rename-r1 22:22; detected rebinds are failures, never success.
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub(super) fn capture(cli: &Path, daemon: &Path) -> Result<Value, String> {
    let cli = cli.canonicalize().map_err(|e| format!("CLI 경로 확인 실패: {e}"))?;
    let daemon = daemon.canonicalize().map_err(|e| format!("데몬 경로 확인 실패: {e}"))?;
    if !cli.is_file() || !daemon.is_file() || !cli.is_absolute() || !daemon.is_absolute()
        || cli.file_name().and_then(|n| n.to_str()) != Some("wave.exe")
        || daemon.file_name().and_then(|n| n.to_str()) != Some("waved.exe")
        || cli.parent() != daemon.parent()
    {
        return Err("동일 설치 폴더의 정식 wave.exe/waved.exe를 확인하지 못했습니다.".into());
    }
    let identity = run("identity", &json!({}))?;
    let user = identity["user"].as_str().filter(|u| !u.is_empty())
        .ok_or("현재 Windows 사용자 확인 실패")?;
    let expected_new = cys::windows_autostart::render_task(&daemon, user);
    let expected_old = cys::windows_autostart::render_task(&daemon.with_file_name("cysd.exe"), user)
        .replace("Wave Terminal daemon (waved)", "CYSJavis terminal daemon (cysd)");
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("이관 식별자 생성 실패: {e}"))?;
    let attempt_id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    run("capture", &json!({"cli":cli,"daemon":daemon,"attempt_id":attempt_id,
        "expected_new":expected_new,"expected_old":expected_old}))
}

pub(super) fn prepare(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    // Exact unique-source XML is durable before TASK_CREATE/UPDATE can become observable.
    persist(snapshot)?;
    run("prepare", snapshot)?;
    snapshot["new_prepared"] = json!(true);
    persist(snapshot)
}
pub(super) fn disable_legacy(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    snapshot["legacy_disable_intent"] = json!(true);
    persist(snapshot)?;
    run("disable", snapshot)?;
    snapshot["legacy_disabled"] = json!(true);
    persist(snapshot)
}
pub(super) fn activate(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    snapshot["activation_intent"] = json!(true);
    persist(snapshot)?;
    run("activate", snapshot)?;
    snapshot["activation_succeeded"] = json!(true);
    persist(snapshot)
}
pub(super) fn rollback(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    snapshot["rollback_intent"] = json!(true);
    persist(snapshot)?;
    let receipt = run("rollback", snapshot)?;
    let preserved_error = receipt["preserved_task_error"].as_str().map(str::to_string);
    if let Some(fields) = receipt.as_object() {
        for (key, value) in fields { snapshot[key] = value.clone(); }
    }
    persist(snapshot)?;
    if let Some(error) = preserved_error { return Err(error); }
    snapshot["rollback_succeeded"] = json!(true);
    persist(snapshot)
}

fn run(operation: &str, snapshot: &Value) -> Result<Value, String> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("WAVE_REGISTRATION_OPERATION", operation)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let input = serde_json::to_vec(snapshot).map_err(|e| format!("등록 백업 변환 실패: {e}"))?;
    let mut child = command.spawn().map_err(|e| format!("작업 스케줄러 확인 도구 실행 실패: {e}"))?;
    let write = child.stdin.take().ok_or("등록 도구 입력 연결 실패")?.write_all(&input);
    let output = child.wait_with_output().map_err(|e| format!("등록 도구 종료 확인 실패: {e}"))?;
    write.map_err(|e| format!("등록 백업 전달 실패: {e}"))?;
    if !output.status.success() {
        return Err(format!("Windows 자동기동 등록 전환 실패: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("작업 스케줄러 응답 확인 실패: {e}"))
}

// Static script plus JSON stdin: Korean paths and task XML never become PowerShell source.
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
  $s = [Console]::In.ReadToEnd() | ConvertFrom-Json
  $reply = @{}
  $op = $env:WAVE_REGISTRATION_OPERATION
  if ($op -eq 'identity') {
    @{user=[System.Security.Principal.WindowsIdentity]::GetCurrent().Name} | ConvertTo-Json -Compress
    exit 0
  }
  $service = New-Object -ComObject 'Schedule.Service'
  $service.Connect()
  $folder = $service.GetFolder('\')
  function Task($name) {
    try { return $folder.GetTask($name) } catch {
      $e = $_.Exception
      while ($null -ne $e) {
        if ($e.HResult -eq -2147024894) { return $null }
        $e = $e.InnerException
      }
      throw "작업 '$name' 조회 실패: $($_.Exception.Message)"
    }
  }
  function Xml($text) {
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create((New-Object System.IO.StringReader($text)), $settings)
    $doc = New-Object System.Xml.XmlDocument
    $doc.XmlResolver = $null
    try { $doc.Load($reader) } finally { $reader.Dispose() }
    return ,$doc
  }
  function Definition($text) {
    $doc = Xml $text
    $ns = New-Object System.Xml.XmlNamespaceManager($doc.NameTable)
    $ns.AddNamespace('t', 'http://schemas.microsoft.com/windows/2004/02/mit/task')
    return @{ doc=$doc; ns=$ns }
  }
  function Saved($task) {
    if ($null -eq $task) { return $null }
    return @{ xml=[string]$task.Xml; enabled=[bool]$task.Enabled; state=[int]$task.State }
  }
  function PathKey($path) {
    if ($path.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) { $path = '\\' + $path.Substring(8) }
    elseif ($path.StartsWith('\\?\')) { $path = $path.Substring(4) }
    if ($path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\)') { throw '상대 실행 경로는 등록하지 않습니다.' }
    return [System.IO.Path]::GetFullPath($path).ToUpperInvariant()
  }
  function Sid($name) {
    if ($name -match '^S-1-') { return (New-Object System.Security.Principal.SecurityIdentifier($name)).Value }
    return (New-Object System.Security.Principal.NTAccount($name)).Translate([System.Security.Principal.SecurityIdentifier]).Value
  }
  function Shape($node) {
    # Structural tuples preserve every element/attribute/value; only whitespace is irrelevant.
    $attributes = @($node.Attributes | Where-Object { $_.NamespaceURI -ne 'http://www.w3.org/2000/xmlns/' } |
      Sort-Object NamespaceURI, LocalName | ForEach-Object { @($_.NamespaceURI, $_.LocalName, $_.Value) })
    $children = @($node.ChildNodes | Where-Object { $_.NodeType -eq [System.Xml.XmlNodeType]::Element } |
      ForEach-Object { Shape $_ })
    $value = if ($children.Count -eq 0) { $node.InnerText } else { '' }
    return ,@($node.NamespaceURI, $node.LocalName, $attributes, $value, $children)
  }
  function Comparable($text, $name) {
    $d = Definition $text
    $enabled = $d.doc.SelectSingleNode('/t:Task/t:Settings/t:Enabled', $d.ns)
    if ($null -eq $enabled -or $enabled.InnerText -notin @('true','false')) { throw "'$name' 활성 상태 정의가 잘못됐습니다." }
    $enabled.InnerText = 'false'
    $uri = $d.doc.SelectSingleNode('/t:Task/t:RegistrationInfo/t:URI', $d.ns)
    if ($null -ne $uri) {
      if ($uri.InnerText -cne "\$name") { throw "다른 등록 경로 '$name' 작업은 변경하지 않습니다." }
      [void]$uri.ParentNode.RemoveChild($uri)
    }
    $source = $d.doc.SelectSingleNode('/t:Task/t:RegistrationInfo/t:Source', $d.ns)
    if ($null -ne $source) {
      if ($source.InnerText -cnotmatch '^WaveTerminal/rename/[a-f0-9]{32}$') { throw '알 수 없는 작업 출처는 변경하지 않습니다.' }
      [void]$source.ParentNode.RemoveChild($source)
    }
    foreach ($node in $d.doc.SelectNodes('//t:UserId', $d.ns)) { $node.InnerText = Sid $node.InnerText }
    foreach ($node in $d.doc.SelectNodes('/t:Task/t:Actions/t:Exec/t:Command', $d.ns)) { $node.InnerText = PathKey $node.InnerText }
    return (ConvertTo-Json -InputObject (Shape $d.doc.DocumentElement) -Depth 80 -Compress)
  }
  function Expected($name) {
    $text = if ($name -eq 'waved') { $s.expected_new } else { $s.expected_old }
    if ([string]::IsNullOrEmpty($text)) { throw '신뢰할 수 있는 등록 원본이 없습니다.' }
    $definition = $service.NewTask(0)
    $definition.XmlText = $text
    return [string]$definition.XmlText
  }
  function Validate($name, $task) {
    if ($null -eq $task) { return }
    if ((Comparable $task.Xml $name) -cne (Comparable (Expected $name) $name)) {
      throw "다른 설치 또는 사용자 지정 '$name' 작업을 변경하지 않습니다. 트리거·권한·설정 확인이 필요합니다."
    }
  }
  function Source($text) {
    $d = Definition $text
    $node = $d.doc.SelectSingleNode('/t:Task/t:RegistrationInfo/t:Source', $d.ns)
    if ($null -eq $node) { return '' }
    return [string]$node.InnerText
  }
  function OwnedNew($task) {
    if ($null -eq $task -or (Source $task.Xml) -cne $s.stage_source -or
        (Comparable $task.Xml 'waved') -cne (Comparable $s.staged_xml 'waved')) {
      throw '이 시도가 준비한 정식 작업의 출처·정의를 확인하지 못했습니다.'
    }
  }
  function Unchanged($name, $saved) {
    $task = Task $name
    Validate $name $task
    if ($null -eq $saved) {
      if ($null -ne $task) { throw "캡처 후 '$name' 작업이 새로 생겼습니다. 전환을 중단합니다." }
    } elseif ($null -eq $task -or (Comparable $task.Xml $name) -cne (Comparable $saved.xml $name)
        -or (Source $task.Xml) -cne (Source $saved.xml)) {
      throw "캡처 후 '$name' 작업 내용이 바뀌었습니다. 전환을 중단합니다."
    }
    return $task
  }
  function Schtasks($arguments) {
    $log = & "$env:SystemRoot\System32\schtasks.exe" @arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "작업 스케줄러 명령 실패: $log" }
  }
  if ($op -eq 'capture') {
    $old = Task 'cysd'; $new = Task 'waved'
    Validate 'cysd' $old; Validate 'waved' $new
    $staged = $service.NewTask(0)
    $staged.XmlText = Expected 'waved'
    $staged.Settings.Enabled = $false
    if ($s.attempt_id -cnotmatch '^[a-f0-9]{32}$') { throw '이관 식별자 오류' }
    $stageSource = 'WaveTerminal/rename/' + $s.attempt_id
    $staged.RegistrationInfo.Source = $stageSource
    @{cli=$s.cli; daemon=$s.daemon; attempt_id=$s.attempt_id; stage_source=$stageSource;
      old=(Saved $old); new=(Saved $new);
      expected_new=$s.expected_new; expected_old=$s.expected_old; staged_xml=[string]$staged.XmlText;
      legacy_registration=($null -ne $old); canonical_registration=($null -ne $new)} | ConvertTo-Json -Depth 12 -Compress
    exit 0
  }
  if ($op -eq 'prepare') {
    [void](Unchanged 'cysd' $s.old)
    $new = Task 'waved'
    if ($null -ne $new -and (Source $new.Xml) -ceq $s.stage_source) {
      OwnedNew $new
      if ($new.Enabled) { throw '이미 활성화된 이관 작업을 다시 준비하지 않습니다.' }
    } else {
      $new = Unchanged 'waved' $s.new
      if ($null -ne $new -and $new.State -notin @(1,3)) { throw '실행 중이거나 대기 중인 정식 작업을 덮어쓰지 않습니다.' }
      Validate 'waved' @{Xml=$s.staged_xml}
      $stage = $service.NewTask(0)
      $stage.XmlText = $s.staged_xml
      if ($stage.Settings.Enabled -or $stage.RegistrationInfo.Source -cne $s.stage_source) { throw '준비 출처 또는 비활성 상태가 잘못됐습니다.' }
      $flags = if ($null -eq $new) { 2 } else { 4 }
      [void]$folder.RegisterTask('waved', $s.staged_xml, $flags, $null, $null, 3, $null)
    }
    $prepared = Task 'waved'
    OwnedNew $prepared
    if ($null -eq $prepared -or $prepared.Enabled) { throw '정식 등록을 비활성 상태로 준비하지 못했습니다.' }
  } elseif ($op -eq 'disable') {
    $old = Unchanged 'cysd' $s.old
    if ($null -ne $old) {
      Schtasks -arguments @('/Change','/TN','cysd','/DISABLE')
      if ((Task 'cysd').Enabled) { throw '기존 자동 재기동을 중지하지 못했습니다.' }
    }
  } elseif ($op -eq 'activate') {
    if ($s.old_owner_stopped -ne $true) { throw '이전 소유자 종료 증거가 없어 자동기동 전환을 보류합니다.' }
    $old = Task 'cysd'
    if ($null -ne $old -or $s.activation_intent -ne $true) { $old = Unchanged 'cysd' $s.old }
    $new = Task 'waved'
    OwnedNew $new
    if ($null -eq $new) { throw '정식 등록 준비 상태를 확인하지 못했습니다.' }
    if ((Comparable $new.Xml 'waved') -cne (Comparable $s.staged_xml 'waved')) { throw '준비 후 정식 등록 내용이 바뀌었습니다.' }
    if ($null -ne $old) {
      if ($old.Enabled) { throw '기존 자동 재기동이 아직 활성 상태입니다.' }
      Schtasks -arguments @('/Delete','/TN','cysd','/F')
    }
    # Enabling this exact staged definition preserves the unique durable ownership nonce.
    if (!$new.Enabled) { Schtasks -arguments @('/Change','/TN','waved','/ENABLE') }
    $registered = Task 'waved'
    OwnedNew $registered
    if ($null -eq $registered -or !$registered.Enabled -or $null -ne (Task 'cysd')) { throw '정식 자동기동 등록 검증 실패' }
    if ($registered.State -notin @(2,4)) { Schtasks -arguments @('/Run','/TN','waved') }
  } elseif ($op -eq 'rollback') {
    if ($s.new_owner_stopped -ne $true) { throw '새 소유자 종료 증거가 없어 등록 복원을 보류합니다.' }
    foreach ($name in @('waved','cysd')) {
      $saved = if ($name -eq 'waved') { $s.new } else { $s.old }
      $current = Task $name
      if ($name -eq 'waved') {
        try {
          Validate $name $current
          if ($null -ne $current) {
            if ($null -ne $saved -and (Source $current.Xml) -ceq (Source $saved.xml) -and
                (Comparable $current.Xml $name) -ceq (Comparable $saved.xml $name) -and $current.Enabled -eq [bool]$saved.enabled) { continue }
            OwnedNew $current
          }
        } catch {
          # A foreign disabled stage cannot launch a second owner; preserve it and recover old.
          if ($null -ne $current -and ($current.Enabled -or $current.State -in @(2,4))) {
            throw '알 수 없는 새 작업이 활성 상태라 기존 작업 재시작을 보류합니다. 두 데몬을 시작하지 않습니다.'
          }
          $reply.foreign_new_task_preserved = $true
          $reply.preserved_task_error = '알 수 없는 새 작업을 보존했습니다. 기존 등록을 복원했으며 업데이트는 완료하지 않았습니다.'
          continue
        }
      } else {
        Validate $name $current
      }
      if ($null -eq $saved) {
        if ($null -ne $current) {
          if ($name -ne 'waved') {
            throw "백업 또는 준비 원본에 없는 '$name' 작업은 삭제하지 않습니다."
          }
          Schtasks -arguments @('/Change','/TN','waved','/DISABLE')
          Schtasks -arguments @('/Delete','/TN','waved','/F')
          if ($null -ne (Task 'waved')) { throw '준비한 정식 등록을 회수하지 못했습니다.' }
        }
      } else {
        if ($name -eq 'cysd' -and $null -ne $current -and
            ((Comparable $current.Xml $name) -cne (Comparable $saved.xml $name) -or (Source $current.Xml) -cne (Source $saved.xml))) {
          throw "알 수 없는 '$name' 변경은 복원으로 덮어쓰지 않습니다."
        }
        if ($name -eq 'cysd' -and $s.legacy_disable_intent -ne $true) {
          if ($null -ne $current -and $current.Enabled -eq [bool]$saved.enabled) { continue }
          throw '이 시도가 중지하지 않은 기존 작업을 복원으로 변경하지 않습니다.'
        }
        # Interactive-token XML captured from this same user requires no stored password.
        [void]$folder.RegisterTask($name, $saved.xml, 6, $null, $null, 3, $null)
        $restored = Task $name
        $restored.Enabled = [bool]$saved.enabled
        if ((Comparable $restored.Xml $name) -cne (Comparable $saved.xml $name)
            -or (Source $restored.Xml) -cne (Source $saved.xml) -or $restored.Enabled -ne [bool]$saved.enabled) {
          throw "'$name' 등록 복원 검증 실패"
        }
      }
    }
    $reply.old_registration_restored = $true
  } else { throw '알 수 없는 등록 전환 단계입니다.' }
  $reply | ConvertTo-Json -Depth 12 -Compress
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;
