use std::collections::HashMap;

use convert_case::Casing;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{
    parse::{discouraged::Speculative, Parse, ParseStream},
    punctuated::Punctuated,
    token::{And, Comma, SelfValue},
    Attribute, Binding, FnArg, Ident, ImplItem, ItemEnum, ItemImpl, ItemStruct, Pat, Receiver,
    ReturnType, Signature, Token,
};

// fn read_exact_ident<'a>(ident_name: &'a str, input: &ParseStream) -> syn::Result<&'a str> {
//     input.step(|cursor| {
//         if let Some((ident, rest)) = cursor.ident() {
//             if ident == ident_name {
//                 return Ok((ident, rest));
//             }
//         }
//         Err(cursor.error(format!("expected `{ident_name}`")))
//     })?;

//     Ok(ident_name)
// }

enum UserErrorType {
    Struct(ItemStruct),
    Enum(ItemEnum),
    TypeBinding(Binding),
}

impl Parse for UserErrorType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input_fork_0 = input.fork();
        let input_fork_1 = input.fork();
        let input_fork_2 = input.fork();
        if let Ok(item_struct) = input_fork_0.parse::<ItemStruct>() {
            input.advance_to(&input_fork_0);
            Ok(UserErrorType::Struct(item_struct))
        } else if let Ok(item_enum) = input_fork_1.parse::<ItemEnum>() {
            input.advance_to(&input_fork_1);
            Ok(UserErrorType::Enum(item_enum))
        } else if let Ok(item_type_binding) = input_fork_2.parse::<Binding>() {
            input.advance_to(&input_fork_2);
            Ok(UserErrorType::TypeBinding(item_type_binding))
        } else {
            Err(input.error("expected struct declaration, enum declaration, or type binding"))
        }
    }
}

impl ToTokens for UserErrorType {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let generated_tokens = match self {
            UserErrorType::Struct(item_struct) => {
                quote! {
                    #item_struct
                }
            }
            UserErrorType::Enum(item_enum) => {
                quote! {
                    #item_enum
                }
            }
            UserErrorType::TypeBinding(type_binding) => {
                quote! {
                    #type_binding
                }
            }
        };
        tokens.extend(quote! {
            #generated_tokens
        });
    }
}

fn is_attribute_worker_fn(attr: &Attribute) -> bool {
    let mut path_segments_iter = attr.path.segments.iter();
    if let Some(first_segment) = path_segments_iter.next() {
        if first_segment.ident == "method_taskifier_fn" {
            return path_segments_iter.next().is_none();
        }
    }

    false
}

struct AsyncWorkerArg {
    key: Ident,
    value: Ident,
}

impl Parse for AsyncWorkerArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key = input.parse::<Ident>()?;
        let key_string = key.to_string();
        let value = if key_string == "debug" || key_string == "use_serde" {
            key.clone()
        } else {
            input.parse::<Token![=]>()?;
            input.parse::<Ident>()?
        };

        Ok(Self { key, value })
    }
}

struct AsyncWorkerArgs {
    module_name: String,
    use_serde: bool,
    debug_mode: bool,
}

impl Parse for AsyncWorkerArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let required_params = ["module_name"];
        let optional_params = ["use_serde", "debug"];
        let mut read_params = HashMap::<String, (i32, AsyncWorkerArg)>::new();

        while !input.is_empty() {
            let arg = input.parse::<AsyncWorkerArg>()?;

            let key_str = arg.key.to_string();
            if !required_params.contains(&key_str.as_str())
                && !optional_params.contains(&key_str.as_str())
            {
                return Err(input.error(format!("unexpected parameter `{}`", key_str)));
            }
            if read_params
                .entry(key_str.clone())
                .or_insert_with_key(|_key| (0, arg))
                .0
                > 1
            {
                return Err(input.error(format!("multiple parameter: `{}`", key_str)));
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            module_name: read_params
                .get("module_name")
                .ok_or_else(|| input.error("missing parameter: `module_name`"))?
                .1
                .value
                .to_string(),
            use_serde: read_params.contains_key("use_serde"),
            debug_mode: read_params.contains_key("debug"),
        })
    }
}

struct FnSignatures {
    items: Vec<Signature>,
}

fn signature_inputs_as_parameters_with_types(signature: &Signature) -> Punctuated<FnArg, Comma> {
    signature
        .inputs
        .iter()
        .filter_map(|input_arg| match input_arg {
            FnArg::Typed(_) => Some(input_arg.clone()),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn signature_inputs_as_parameter_names(signature: &Signature) -> Punctuated<Box<Pat>, Comma> {
    signature
        .inputs
        .iter()
        .filter_map(|input_arg| match input_arg {
            FnArg::Typed(arg) => Some(arg.pat.clone()),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn function_name_as_ident(function_ident: &Ident, suffix: &str) -> Ident {
    let task_name = function_ident
        .to_string()
        .to_case(convert_case::Case::Pascal);

    Ident::new(&format!("{task_name}{suffix}"), Span::call_site())
}

fn signature_as_task_structs(signature: &Signature, serde_derive: &TokenStream2) -> TokenStream2 {
    let input_args: TokenStream2 = signature
        .inputs
        .iter()
        .filter_map(|input_arg| match input_arg {
            FnArg::Typed(_) => Some(quote! { pub #input_arg, }),
            FnArg::Receiver(_) => None,
        })
        .collect();

    let output_type = match &signature.output {
        ReturnType::Default => {
            quote! {
                ()
            }
        }
        ReturnType::Type(_, boxed_type) => {
            quote! {
                #boxed_type
            }
        }
    };

    let params_struct_ident = function_name_as_ident(&signature.ident, "Params");
    let result_struct_ident = function_name_as_ident(&signature.ident, "Result");

    quote! {
        #serde_derive
        pub struct #params_struct_ident {
            #input_args
        }

        #serde_derive
        pub struct #result_struct_ident(pub #output_type);
    }
}

fn signature_as_task_enum_variant(signature: &Signature) -> TokenStream2 {
    let task_variant_ident = function_name_as_ident(&signature.ident, "");

    let params_struct_ident = function_name_as_ident(&signature.ident, "Params");

    quote! {
        #task_variant_ident(#params_struct_ident)
    }
}

fn signature_as_task_result_enum_variant(signature: &Signature) -> TokenStream2 {
    let task_variant_ident = function_name_as_ident(&signature.ident, "");

    let result_struct_ident = function_name_as_ident(&signature.ident, "Result");

    quote! {
        #task_variant_ident(#result_struct_ident)
    }
}

fn signature_as_as_task_result_funcion(signature: &Signature) -> TokenStream2 {
    let task_variant_ident = function_name_as_ident(&signature.ident, "");
    let result_struct_ident = function_name_as_ident(&signature.ident, "Result");
    let function_name_ident =
        Ident::new(&format!("as_{}_result", signature.ident), Span::call_site());
    quote! {
        pub fn #function_name_ident(&self) -> Option<&self::#result_struct_ident> {
            if let self::TaskResult::#task_variant_ident(result) = self {
                Some(result)
            } else {
                None
            }
        }
    }
}

fn signature_as_task_builder_function(signature: &Signature) -> TokenStream2 {
    let function_ident = &signature.ident;
    let task_variant_ident = function_name_as_ident(&signature.ident, "");
    let task_params_ident = function_name_as_ident(&signature.ident, "Params");

    let fn_parameters_with_types = signature_inputs_as_parameters_with_types(signature);
    let fn_parameter_names = signature_inputs_as_parameter_names(signature);

    quote! {
        pub fn #function_ident(#fn_parameters_with_types) -> self::Task {
            self::Task::#task_variant_ident(
                self::#task_params_ident {
                    #fn_parameter_names
                }
            )
        }
    }
}

fn signature_as_channeled_task_enum_variant(signature: &Signature) -> TokenStream2 {
    let task_variant_ident = function_name_as_ident(&signature.ident, "");
    let params_struct_ident = function_name_as_ident(&signature.ident, "Params");
    let result_struct_ident = function_name_as_ident(&signature.ident, "Result");

    quote! {
        #task_variant_ident {
            params: #params_struct_ident,
            result_sender: ::tokio::sync::oneshot::Sender<#result_struct_ident>,
        }
    }
}

struct ClientFunction {
    head: TokenStream2,
    body: TokenStream2,
}

fn signature_as_client_function(signature: &Signature) -> ClientFunction {
    let mut client_signature = signature.clone();

    client_signature.asyncness = None;

    // create empty inputs
    client_signature.inputs = Punctuated::new();
    // add &self as first parameter
    client_signature.inputs.push(FnArg::Receiver(Receiver {
        attrs: vec![],
        reference: Some((
            And {
                spans: [Span::call_site()],
            },
            None,
        )),
        mutability: None,
        self_token: SelfValue {
            span: Span::call_site(),
        },
    }));
    // add every non self (non receiver) type parameters
    client_signature
        .inputs
        .extend(signature_inputs_as_parameters_with_types(signature));

    // empty client signature, and construct a TokenStream instead
    client_signature.output = ReturnType::Default;
    let client_fn_output_type = match &signature.output {
        ReturnType::Default => {
            quote! {
                Result<(), ::method_taskifier::AllWorkersDroppedError>
            }
        }
        ReturnType::Type(_, boxed_type) => {
            quote! {
                Result<#boxed_type, ::method_taskifier::AllWorkersDroppedError>
            }
        }
    };

    let fn_parameter_names = signature_inputs_as_parameter_names(signature);

    let fn_name = signature.ident.to_string();
    let task_variant_ident = function_name_as_ident(&signature.ident, "");

    let error_string_send = format!("{{}}::Client::{}, msg = {{:?}}", fn_name);
    let error_string_recv = format!("{{}}::Client::{} response, msg = {{:?}}", fn_name);

    let head = quote! {
        pub #client_signature -> impl ::std::future::Future<Output = #client_fn_output_type>
    };

    let task_struct_ident = function_name_as_ident(&signature.ident, "Params");

    let body = quote! {
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();

        let ret = self.task_sender.send(ChanneledTask::#task_variant_ident {
            params: #task_struct_ident {
                #fn_parameter_names
            },
            result_sender,
        });

        async move {
            if let Err(e) = ret {
                ::log::error!(#error_string_send, module_path!(), e);
                return Err(::method_taskifier::AllWorkersDroppedError);
            }

            match result_receiver.await {
                Ok(ret) => Ok(ret.0),
                Err(e) => {
                    ::log::error!(#error_string_recv, module_path!(), e);
                    Err(::method_taskifier::AllWorkersDroppedError)
                },
            }
        }
    };

    ClientFunction { head, body }
}

fn signature_as_client_execute_task_match_line(
    signature: &Signature,
    module_ident: &Ident,
) -> TokenStream2 {
    let fn_name = signature.ident.to_string();
    let task_variant_ident = function_name_as_ident(&signature.ident, "");

    let error_string_send = format!("{{}}::Client::{}, msg = {{:?}}", fn_name);
    let error_string_recv = format!("{{}}::Client::{} response, msg = {{:?}}", fn_name);

    quote! {
        self::#module_ident::Task::#task_variant_ident(params) => {
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            let ret = self.task_sender.send(self::#module_ident::ChanneledTask::#task_variant_ident { params, result_sender });
            if let Err(e) = ret {
                ::log::error!(#error_string_send, module_path!(), e);
                return Err(::method_taskifier::AllWorkersDroppedError);
            }
            match result_receiver.await {
                Ok(ret) => Ok(self::#module_ident::TaskResult::#task_variant_ident(ret)),
                Err(e) => {
                    ::log::error!(#error_string_recv, module_path!(), e);
                    Err(::method_taskifier::AllWorkersDroppedError)
                }
            }
        }
    }
}

fn signature_as_task_execution_match_line(
    signature: &Signature,
    module_ident: &Ident,
) -> TokenStream2 {
    let function_ident = &signature.ident;
    let task_ident = function_name_as_ident(function_ident, "");
    let task_params_type_ident = function_name_as_ident(function_ident, "Params");
    let task_result_type_ident = function_name_as_ident(function_ident, "Result");

    let fn_parameter_names = signature_inputs_as_parameter_names(signature);

    let awaitness = if signature.asyncness.is_some() {
        quote! {
            .await
        }
    } else {
        quote! {}
    };

    quote! {
        self::#module_ident::Task::#task_ident(
            self::#module_ident::#task_params_type_ident {
                #fn_parameter_names
            }
        ) => {
            let ret = self.#function_ident(#fn_parameter_names) #awaitness;
            self::#module_ident::TaskResult::#task_ident(self::#module_ident::#task_result_type_ident(ret))
        }
    }
}

fn signature_as_channeled_task_execution_match_line(
    signature: &Signature,
    module_ident: &Ident,
) -> TokenStream2 {
    let function_ident = &signature.ident;
    let task_ident = function_name_as_ident(function_ident, "");
    let task_params_type_ident = function_name_as_ident(function_ident, "Params");
    let task_result_type_ident = function_name_as_ident(function_ident, "Result");

    let fn_parameter_names = signature_inputs_as_parameter_names(signature);

    let awaitness = if signature.asyncness.is_some() {
        quote! {
            .await
        }
    } else {
        quote! {}
    };

    quote! {
        self::#module_ident::ChanneledTask::#task_ident {
            result_sender,
            params: self::#module_ident::#task_params_type_ident {
                #fn_parameter_names
            }
        } => {
            let ret = self.#function_ident(#fn_parameter_names) #awaitness;
            let _ = result_sender.send(self::#module_ident::#task_result_type_ident(ret));
        }
    }
}

impl FnSignatures {
    fn as_task_structs(&self, serde_derive: &TokenStream2) -> TokenStream2 {
        let mut ret = TokenStream2::new();

        ret.extend(
            self.items
                .iter()
                .map(|item| signature_as_task_structs(item, serde_derive)),
        );

        ret
    }

    fn as_task_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(self.items.iter().map(signature_as_task_enum_variant));

        ret
    }

    fn as_task_result_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(self.items.iter().map(signature_as_task_result_enum_variant));

        ret
    }

    fn as_as_task_result_functions(&self) -> TokenStream2 {
        self.items
            .iter()
            .map(signature_as_as_task_result_funcion)
            .collect()
    }

    fn as_channeled_task_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(
            self.items
                .iter()
                .map(signature_as_channeled_task_enum_variant),
        );

        ret
    }

    fn task_builder_functions(&self) -> TokenStream2 {
        self.items
            .iter()
            .map(signature_as_task_builder_function)
            .collect()
    }

    fn as_client_impl(&self, module_ident: &Ident) -> TokenStream2 {
        let client_functions: Vec<ClientFunction> = self
            .items
            .iter()
            .map(signature_as_client_function)
            .collect();

        let client_function_definitions: TokenStream2 = client_functions
            .iter()
            .map(|client_fn| {
                let head = &client_fn.head;
                let body = &client_fn.body;
                quote! {
                    #head {
                        #body
                    }
                }
            })
            .collect();

        let execute_task_match_lines: TokenStream2 = self
            .items
            .iter()
            .map(|item| signature_as_client_execute_task_match_line(item, module_ident))
            .collect();

        quote! {
            #[derive(Clone)]
            pub struct Client {
                task_sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
            }

            impl Client {
                pub fn new(
                    sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
                ) -> Self {
                    Self {
                        task_sender: sender,
                    }
                }

                pub async fn execute_task(
                    &self,
                    task: self::#module_ident::Task
                ) -> Result<self::#module_ident::TaskResult, ::method_taskifier::AllWorkersDroppedError> {
                    match task {
                        #execute_task_match_lines
                    }
                }

                #client_function_definitions
            }
        }
    }

    fn as_task_execution_match(&self, module_ident: &Ident) -> TokenStream2 {
        let task_execution_match_lines: TokenStream2 = self
            .items
            .iter()
            .map(|signature| signature_as_task_execution_match_line(signature, module_ident))
            .collect();

        quote! {
            match task {
                #task_execution_match_lines
            }
        }
    }

    fn as_channeled_task_execution_match(&self, module_ident: &Ident) -> TokenStream2 {
        let channeled_task_execution_match_lines: TokenStream2 = self
            .items
            .iter()
            .map(|signature| {
                signature_as_channeled_task_execution_match_line(signature, module_ident)
            })
            .collect();

        quote! {
            match task {
                #channeled_task_execution_match_lines
            }
        }
    }
}

struct WorkerTaskExecutorImpl {
    head: TokenStream2,
    body: TokenStream2,
}

impl WorkerTaskExecutorImpl {
    pub fn new(item_impl: &ItemImpl, module_ident: &Ident, fn_signatures: &FnSignatures) -> Self {
        let attrs = &item_impl.attrs;
        let defaultness = &item_impl.defaultness;
        let unsafety = &item_impl.unsafety;
        let impl_token = &item_impl.impl_token;
        let generics = &item_impl.generics;
        let trait_ = &item_impl.trait_;
        let self_ty = &item_impl.self_ty;

        let has_async = fn_signatures
            .items
            .iter()
            .any(|signature| signature.asyncness.is_some());

        let executor_asyncness = if has_async {
            quote! {
                async
            }
        } else {
            quote! {}
        };

        let executor_call_awaitness = if has_async {
            quote! {
                .await
            }
        } else {
            quote! {}
        };

        let attributes_ts: TokenStream2 = attrs
            .iter()
            .map(|attr| {
                quote! {
                    #attr
                }
            })
            .collect();

        let trait_ts = match trait_ {
            Some((bang, path, for_)) => {
                quote! {
                    #bang #path #for_
                }
            }
            None => quote! {},
        };

        let task_execution_match = fn_signatures.as_task_execution_match(module_ident);
        let channeled_task_execution_match =
            fn_signatures.as_channeled_task_execution_match(module_ident);

        Self {
            head: quote! {
                #attributes_ts
                #defaultness
                #unsafety #impl_token #generics #trait_ts #self_ty
            },
            body: quote! {
                pub #executor_asyncness fn execute_task(
                    &mut self,
                    task: self::#module_ident::Task,
                ) -> self::#module_ident::TaskResult {
                    #task_execution_match
                }

                pub #executor_asyncness fn execute_channeled_task(
                    &mut self,
                    task: self::#module_ident::ChanneledTask,
                ) {
                    #channeled_task_execution_match
                }

                pub #executor_asyncness fn try_execute_channeled_task_from_queue(
                    &mut self,
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::#module_ident::ChanneledTask>,
                ) -> Result<bool, ::method_taskifier::AllClientsDroppedError> {
                    let task = receiver.try_recv();
                    match task {
                        Ok(task) => {
                            self.execute_channeled_task(task) #executor_call_awaitness;
                            return Ok(true);
                        }
                        Err(::method_taskifier::task_channel::TryRecvError::Empty) => return Ok(false),
                        Err(::method_taskifier::task_channel::TryRecvError::Disconnected) => {
                            return Err(::method_taskifier::AllClientsDroppedError)
                        }
                    }
                }

                pub async fn execute_channeled_task_from_queue(
                    &mut self,
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::#module_ident::ChanneledTask>,
                ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
                    let task = receiver.recv_async().await;
                    match task {
                        Ok(task) => {
                            self.execute_channeled_task(task) #executor_call_awaitness;
                            Ok(())
                        }
                        Err(::method_taskifier::task_channel::RecvError::Disconnected) => {
                            Err(::method_taskifier::AllClientsDroppedError)
                        }
                    }
                }

                pub #executor_asyncness fn execute_remaining_channeled_tasks_from_queue(
                    &mut self,
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::#module_ident::ChanneledTask>,
                ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
                    while self.try_execute_channeled_task_from_queue(receiver) #executor_call_awaitness ? {}
                    Ok(())
                }

                pub async fn execute_channeled_tasks_from_queue_until_clients_dropped(
                    &mut self,
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::#module_ident::ChanneledTask>,
                ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
                    loop {
                        self.execute_channeled_task_from_queue(receiver).await?
                    }
                }
            },
        }
    }
}

impl ToTokens for WorkerTaskExecutorImpl {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let head = &self.head;
        let body = &self.body;

        tokens.extend(quote! {
            #head {
                #body
            }
        });
    }
}

struct ImplBlock {
    item_impl: ItemImpl,
    fn_items: FnSignatures,
}

impl Parse for ImplBlock {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // let input_fork_0 = input.fork();
        let mut item_impl: ItemImpl = input.parse()?;

        let fn_items = FnSignatures {
            items: item_impl
                .items
                .iter_mut()
                .filter_map(|item| {
                    if let ImplItem::Method(method) = item {
                        let mut found_method_taskifier_fn_attribute = false;
                        method.attrs.retain(|attr| {
                            if is_attribute_worker_fn(attr) {
                                found_method_taskifier_fn_attribute = true;
                                false
                            } else {
                                true
                            }
                        });

                        if found_method_taskifier_fn_attribute {
                            Some(method.sig.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect(),
        };

        // for sig in fn_items.items.iter() {
        //     if sig.asyncness.is_some() {
        //         return Err(input_fork_0.error(format!(
        //             "a worker function cannot be async `{}`",
        //             sig.to_token_stream(),
        //         )));
        //     }
        // }

        Ok(Self {
            item_impl,
            fn_items,
        })
    }
}

#[proc_macro_attribute]
pub fn method_taskifier_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let args: AsyncWorkerArgs = syn::parse_macro_input!(args);
    let impl_block: ImplBlock = syn::parse_macro_input!(input);

    let serde_derive = if args.use_serde {
        quote! {
            #[derive(::serde::Serialize, ::serde::Deserialize)]
        }
    } else {
        quote! {}
    };

    let module_ident = Ident::new(&args.module_name, Span::call_site());
    let worker_task_executor_impl =
        WorkerTaskExecutorImpl::new(&impl_block.item_impl, &module_ident, &impl_block.fn_items);
    let task_structs: TokenStream2 = impl_block.fn_items.as_task_structs(&serde_derive);
    let task_declarations: Punctuated<TokenStream2, Comma> =
        impl_block.fn_items.as_task_enum_variants();
    let task_result_declarations: Punctuated<TokenStream2, Comma> =
        impl_block.fn_items.as_task_result_enum_variants();
    let as_task_result_functions = impl_block.fn_items.as_as_task_result_functions();
    let channeled_task_declarations: Punctuated<TokenStream2, Comma> =
        impl_block.fn_items.as_channeled_task_enum_variants();
    let client_impl = impl_block.fn_items.as_client_impl(&module_ident);
    let task_builder_functions = impl_block.fn_items.task_builder_functions();

    let item_impl = &impl_block.item_impl;

    let tokenstream = quote! {
        #item_impl

        #worker_task_executor_impl

        pub mod #module_ident {
            use super::*;

            #task_structs

            #serde_derive
            pub enum Task {
                #task_declarations
            }

            impl Task {
                #task_builder_functions
            }

            #serde_derive
            pub enum TaskResult {
                #task_result_declarations
            }

            impl TaskResult {
                #as_task_result_functions
            }

            pub enum ChanneledTask {
                #channeled_task_declarations
            }

            pub fn channel() -> (
                ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
                ::method_taskifier::task_channel::TaskReceiver<ChanneledTask>,
            ) {
                ::method_taskifier::task_channel::task_channel()
            }

            #client_impl
        }
    };

    if args.debug_mode {
        panic!("{tokenstream}");
    }

    tokenstream.into()
}
