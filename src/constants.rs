use phf::phf_map;

pub const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub static CODES: phf::Map<u32, &'static str> = phf_map! {
    100u32 => "OK",
    101u32 => "Unknown error",
    102u32 => "Unsupported version",
    103u32 => "Request not permitted",
    104u32 => "User already logged in",
    105u32 => "User is not logged in",
    106u32 => "Username or password is incorrect",
    107u32 => "User does not have necessary permissions",
    203u32 => "Password is incorrect",
    205u32 => "User does not exist",
    207u32 => "Blacklisted",
    511u32 => "Start of upgrade",
    512u32 => "Upgrade was not started",
    513u32 => "Upgrade data errors",
    514u32 => "Upgrade error",
    515u32 => "Upgrade successful",
};

pub static QCODES: phf::Map<&'static str, u16> = phf_map! {
    "AuthorityList" => 1470,
    "Users" => 1472,
    "Groups" => 1474,
    "AddGroup" => 1476,
    "ModifyGroup" => 1478,
    "DelGroup" => 1480,
    "User" => 1482,
    "ModifyUser" => 1484,
    "DelUser" => 1486,
    "ModifyPassword" => 1488,
    "AlarmInfo" => 1504,
    "AlarmSet" => 1500,
    "ChannelTitle" => 1046,
    "EncodeCapability" => 1360,
    "General" => 1042,
    "KeepAlive" => 1006,
    "OPMachine" => 1450,
    "OPMailTest" => 1636,
    "OPMonitor" => 1413,
    "OPNetKeyboard" => 1550,
    "OPPTZControl" => 1400,
    "OPSNAP" => 1560,
    "OPSendFile" => 0x5F2,
    "OPSystemUpgrade" => 0x5F5,
    "OPTalk" => 1434,
    "OPTalkStart" => 1430,
    "OPTalkData" => 1432,
    "OPTimeQuery" => 1452,
    "OPTimeSetting" => 1450,
    "NetWork.NetCommon" => 1042,
    "OPNetAlarm" => 1506,
    "SystemFunction" => 1360,
    "SystemInfo" => 1020,
};

pub static KEY_CODES: phf::Map<&'static str, &'static str> = phf_map! {
    "M" => "Menu",
    "I" => "Info",
    "E" => "Esc",
    "F" => "Func",
    "S" => "Shift",
    "L" => "Left",
    "U" => "Up",
    "R" => "Right",
    "D" => "Down",
};

pub const OK_CODES: &[u32] = &[100, 515];

pub const TCP_PORT: u16 = 34567;
pub const UDP_PORT: u16 = 34568;
