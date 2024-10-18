// use library::ip_data_types::{NodeType, CmdType};
// use tokio::sync::mpsc::Sender;
// use mini_async_repl::anyhow::{self, Context};
// use mini_async_repl::{
//     command::{
//         lift_validation_err, validate, Command, CommandArgInfo, CommandArgType,
//         ExecuteCommand,
//     },
//     CommandStatus, Repl,
// };
// use std::future::Future;
// use std::pin::Pin;
// 
// async fn send_cmd(command: CmdType, send_nchan: Sender<CmdType>) {
//     match send_nchan.send(command).await {
//         Err(e) => eprintln!("Error: Encountered error while sending command to node: {}", e),
//         _ => (),
//     }
// }
// 
// struct LiCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl LiCommandHandler {
//     pub fn new(send_nchan: Sender<CmdType>) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self) -> anyhow::Result<CommandStatus>{
//         let li_cmd: CmdType = CmdType::Li;
//         send_cmd(li_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// 
// impl ExecuteCommand for LiCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         Box::pin(self.handle_command())
//     }
// }
// 
// struct LnCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl LnCommandHandler {
//     pub fn new(send_nchan: Sender<CmdType>) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self) -> anyhow::Result<CommandStatus>{
//         let ln_cmd: CmdType = CmdType::Ln;
//         send_cmd(ln_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// 
// impl ExecuteCommand for LnCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         Box::pin(self.handle_command())
//     }
// }
// 
// struct LrCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl LrCommandHandler {
//     pub fn new( send_nchan: Sender<CmdType> ) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self) -> anyhow::Result<CommandStatus>{
//         let lr_cmd: CmdType = CmdType::Lr;
//         send_cmd(lr_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// impl ExecuteCommand for LrCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         Box::pin(self.handle_command())
//     }
// }
// 
// struct DownCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl DownCommandHandler {
//     pub fn new( send_nchan: Sender<CmdType> ) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self, ifname: String) -> anyhow::Result<CommandStatus>{
//         let down_cmd: CmdType = CmdType::Down(ifname);
//         send_cmd(down_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// impl ExecuteCommand for DownCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         let ifname: String = String::from(&args[0]);
//         Box::pin(self.handle_command(ifname))
//     }
// }
// 
// struct UpCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl UpCommandHandler {
//     pub fn new( send_nchan: Sender<CmdType> ) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self, ifname: String) -> anyhow::Result<CommandStatus>{
//         let up_cmd: CmdType = CmdType::Up(ifname);
//         send_cmd(up_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// impl ExecuteCommand for UpCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         let ifname: String = String::from(&args[0]);
//         Box::pin(self.handle_command(ifname))
//     }
// }
// 
// struct SendCommandHandler {
//     send_nchan: Sender<CmdType>
// }
// impl SendCommandHandler {
//     pub fn new( send_nchan: Sender<CmdType> ) -> Self {
//         Self { send_nchan }
//     }
//     async fn handle_command(&mut self, addr: String, message: String) -> anyhow::Result<CommandStatus>{
//         let ls_cmd: CmdType = CmdType::Send(addr, message);
//         send_cmd(ls_cmd, self.send_nchan.clone()).await;
//         Ok(CommandStatus::Done)
//     }
// }
// impl ExecuteCommand for SendCommandHandler {
//     fn execute(
//         &mut self,
//         args: Vec<String>,
//         args_info: Vec<CommandArgInfo>,
//     ) -> Pin<Box<dyn Future<Output = anyhow::Result<CommandStatus>> + '_>> {
//         let valid = validate(args.clone(), args_info.clone());
//         if valid.is_err() {
//             return Box::pin(lift_validation_err(valid));
//         }
//         let addr: String = String::from(&args[0]);
//         let mut msg: String = String::from("");
// 
//         let i: usize = 1;
//         while args.len() < i {
//             msg.push_str(&args[i]);
//             if args[i].contains("\n") {
//                 break;
//             }
//         }
//         Box::pin(self.handle_command(addr, msg))
//     }
// }
// 
// #[tokio::main]
// pub async fn run_repl(_n_type: NodeType, send_nchan: Sender<CmdType>) -> anyhow::Result<()> { 
//     #[rustfmt::skip]
//     let mut repl = Repl::builder()
//         .add("li", Command::new(
//             "List interfaces".into(),
//             Vec::new(),
//             Box::new(LiCommandHandler::new(send_nchan.clone())),
//         ))
//         .add("lr", Command::new(
//             "List routes".into(),
//             Vec::new(),
//             Box::new(LrCommandHandler::new(send_nchan.clone())),
//         ))
//         .add("ln", Command::new(
//             "List routes".into(),
//            Vec::new(), 
//             Box::new(LnCommandHandler::new(send_nchan.clone())),
//         ))
//         .add("down", Command::new(
//             "Disable interface <ifname>".into(),
//             vec![CommandArgInfo::new_with_name(CommandArgType::Custom, "ifname")],
//             Box::new(DownCommandHandler::new(send_nchan.clone())),
//         ))
//         .add("up", Command::new(
//             "Enable an interface <ifname>".into(),
//             vec![CommandArgInfo::new_with_name(CommandArgType::Custom, "ifname")],
//             Box::new(UpCommandHandler::new(send_nchan.clone())),
//         ))
//         .add("send", Command::new(
//             "Send a message to an address <addr>, <message>".into(),
//             vec![
//                             CommandArgInfo::new_with_name(CommandArgType::Custom, "addr"),
//                             CommandArgInfo::new_with_name(CommandArgType::Custom, "message")
//                             ],
//             Box::new(SendCommandHandler::new(send_nchan.clone())),
//         ))
//         .build()
//         .context("Failed to create repl")?;
//     repl.run().await.context("Critical REPL error")?;
//     Ok(())
//     
// }
// 