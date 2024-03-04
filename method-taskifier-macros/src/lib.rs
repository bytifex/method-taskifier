use convert_case::Casing;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
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

// fn syn_generics_without_where_clause(generics: &syn::Generics) -> syn::Generics {
//     let mut generics = generics.clone();
//     generics.where_clause = None;
//     generics
// }

// fn syn_generics_where_clause(generics: &syn::Generics) -> Option<&syn::WhereClause> {
//     generics.where_clause.as_ref()
// }

// fn syn_generics_stripped(generics: &syn::Generics) -> syn::Generics {
//     let mut generics = generics.clone();
//     generics.where_clause = None;
//     for param in generics.params.iter_mut() {
//         match param {
//             syn::GenericParam::Type(type_param) => {
//                 type_param.attrs.clear();
//                 type_param.colon_token = None;
//                 type_param.bounds.clear();
//                 type_param.eq_token = None;
//                 type_param.default = None;
//             }
//             syn::GenericParam::Lifetime(lifetime_param) => {
//                 lifetime_param.attrs.clear();
//                 lifetime_param.colon_token = None;
//                 lifetime_param.bounds.clear();
//             }
//             syn::GenericParam::Const(_const_param) => {}
//         }
//     }

//     generics
// }

fn syn_path_to_string(path: &syn::Path) -> String {
    path.leading_colon
        .iter()
        .map(|_| "::".to_string())
        .chain(
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string()),
        )
        .collect::<Vec<String>>()
        .join("::")
}

fn does_attribute_equals_to(attr: &syn::Attribute, attribute_name: &str) -> bool {
    let mut path_segments_iter = attr.path().segments.iter();
    if let Some(first_segment) = path_segments_iter.next() {
        if first_segment.ident == attribute_name {
            return path_segments_iter.next().is_none();
        }
    }

    false
}

fn read_assign_then_ident(input: &ParseStream) -> syn::Result<syn::Ident> {
    input.parse::<syn::Token![=]>()?;
    input.parse::<syn::Ident>()
}

fn read_assign_then_path(input: &ParseStream) -> syn::Result<syn::Path> {
    input.parse::<syn::Token![=]>()?;
    input.parse::<syn::Path>()
}

struct MethodTaskifierImplArgs {
    task_definitions_module_path: syn::Path,
    client_name: syn::Ident,
    use_serde: bool,
    debug_mode: bool,
}

impl Parse for MethodTaskifierImplArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut task_definitions_module_path = None;
        let mut client_name = None;
        let mut use_serde = None;
        let mut debug = None;

        while !input.is_empty() {
            let arg_name = input.parse::<syn::Ident>()?;
            let arg_name_str = arg_name.to_string();

            match arg_name_str.as_str() {
                "task_definitions_module_path" => {
                    if task_definitions_module_path.is_some() {
                        return Err(
                            input.error("multiple parameter: `task_definitions_module_path`")
                        );
                    }

                    task_definitions_module_path = Some(read_assign_then_path(&input)?);
                }
                "client_name" => {
                    if client_name.is_some() {
                        return Err(input.error("multiple parameter: `client_name`"));
                    }

                    client_name = Some(read_assign_then_ident(&input)?);
                }
                "use_serde" => {
                    if use_serde.is_some() {
                        return Err(input.error("multiple parameter: `use_serde`"));
                    }

                    use_serde = Some(());
                }
                "debug" => {
                    if debug.is_some() {
                        return Err(input.error("multiple parameter: `debug`"));
                    }

                    debug = Some(());
                }
                _ => {
                    return Err(input.error(format!("unexpected parameter `{}`", arg_name_str)));
                }
            }

            if !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
            }
        }

        Ok(Self {
            task_definitions_module_path: task_definitions_module_path
                .ok_or_else(|| input.error("missing parameter: `task_definitions_module_path`"))?,
            client_name: client_name
                .ok_or_else(|| input.error("missing parameter: `client_name`"))?,
            use_serde: use_serde.is_some(),
            debug_mode: debug.is_some(),
        })
    }
}

struct ClientMethod {
    sig: syn::Signature,
    body: syn::Block,
}

impl ToTokens for ClientMethod {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let sig = &self.sig;
        let body = &self.body;

        tokens.extend(quote! {
            #sig {
                #body
            }
        });
    }
}

struct WorkerMethodSignature(syn::Signature);

impl WorkerMethodSignature {
    fn function_name_as_pascal_case_ident(&self, suffix: &str) -> syn::Ident {
        let task_name = self.0.ident.to_string().to_case(convert_case::Case::Pascal);

        syn::Ident::new(&format!("{task_name}{suffix}"), Span::call_site())
    }

    fn as_task_structs(&self, serde_derive: &TokenStream2) -> TokenStream2 {
        let input_args: TokenStream2 = self
            .0
            .inputs
            .iter()
            .filter_map(|input_arg| match input_arg {
                syn::FnArg::Typed(_) => Some(quote! { pub #input_arg, }),
                syn::FnArg::Receiver(_) => None,
            })
            .collect();

        let output_type = match &self.0.output {
            syn::ReturnType::Default => {
                quote! {
                    ()
                }
            }
            syn::ReturnType::Type(_, boxed_type) => {
                quote! {
                    #boxed_type
                }
            }
        };

        let params_struct_ident = self.function_name_as_pascal_case_ident("Params");
        let result_struct_ident = self.function_name_as_pascal_case_ident("Result");

        quote! {
            #serde_derive
            pub struct #params_struct_ident {
                #input_args
            }

            #serde_derive
            pub struct #result_struct_ident (pub #output_type);
        }
    }

    fn as_task_enum_variant(&self) -> TokenStream2 {
        let task_variant_ident = self.function_name_as_pascal_case_ident("");
        let params_struct_ident = self.function_name_as_pascal_case_ident("Params");

        quote! {
            #task_variant_ident(#params_struct_ident)
        }
    }

    fn as_task_result_enum_variant(&self) -> TokenStream2 {
        let task_variant_ident = self.function_name_as_pascal_case_ident("");
        let result_struct_ident = self.function_name_as_pascal_case_ident("Result");

        quote! {
            #task_variant_ident(#result_struct_ident)
        }
    }

    fn inputs_as_parameters_with_types(&self) -> Punctuated<syn::FnArg, Comma> {
        self.0
            .inputs
            .iter()
            .filter_map(|input_arg| match input_arg {
                syn::FnArg::Typed(_) => Some(input_arg.clone()),
                syn::FnArg::Receiver(_) => None,
            })
            .collect()
    }

    fn inputs_as_parameter_names(&self) -> Punctuated<Box<syn::Pat>, Comma> {
        self.0
            .inputs
            .iter()
            .filter_map(|input_arg| match input_arg {
                syn::FnArg::Typed(arg) => Some(arg.pat.clone()),
                syn::FnArg::Receiver(_) => None,
            })
            .collect()
    }

    fn as_as_task_result_funcion(&self) -> TokenStream2 {
        let task_variant_ident = self.function_name_as_pascal_case_ident("");
        let result_struct_ident = self.function_name_as_pascal_case_ident("Result");
        let function_name_ident =
            syn::Ident::new(&format!("as_{}_result", self.0.ident), Span::call_site());
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

    fn as_task_builder_function(&self) -> TokenStream2 {
        let function_ident = &self.0.ident;
        let task_variant_ident = self.function_name_as_pascal_case_ident("");
        let task_params_ident = self.function_name_as_pascal_case_ident("Params");

        let fn_parameters_with_types = self.inputs_as_parameters_with_types();
        let fn_parameter_names = self.inputs_as_parameter_names();

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

    fn as_channeled_task_enum_variant(&self) -> TokenStream2 {
        let task_variant_ident = self.function_name_as_pascal_case_ident("");
        let params_struct_ident = self.function_name_as_pascal_case_ident("Params");
        let result_struct_ident = self.function_name_as_pascal_case_ident("Result");

        quote! {
            #task_variant_ident {
                params: #params_struct_ident,
                result_sender: ::tokio::sync::oneshot::Sender<#result_struct_ident>,
            }
        }
    }
    fn as_client_function(&self) -> ClientFunction {
        let mut client_signature = self.0.clone();

        client_signature.asyncness = None;

        // create empty inputs
        client_signature.inputs = Punctuated::new();
        // add &self as first parameter

        let tokens = quote! { &self }.into();
        let receiver = syn::parse(tokens).unwrap();
        client_signature.inputs.push(syn::FnArg::Receiver(receiver));
        // add every non self (non receiver) type parameters
        client_signature
            .inputs
            .extend(self.inputs_as_parameters_with_types());

        // empty client signature, and construct a TokenStream instead
        client_signature.output = syn::ReturnType::Default;
        let client_fn_output_type = match &self.0.output {
            syn::ReturnType::Default => {
                quote! {
                    Result<(), ::method_taskifier::AllWorkersDroppedError>
                }
            }
            syn::ReturnType::Type(_, boxed_type) => {
                quote! {
                    Result<#boxed_type, ::method_taskifier::AllWorkersDroppedError>
                }
            }
        };

        let fn_parameter_names = self.inputs_as_parameter_names();

        let fn_name = self.0.ident.to_string();
        let task_variant_ident = self.function_name_as_pascal_case_ident("");

        let error_string_send = format!("{{}}::Client::{}, msg = {{:?}}", fn_name);
        let error_string_recv = format!("{{}}::Client::{} response, msg = {{:?}}", fn_name);

        let sending_task_log_msg = format!("{{}}::Client::{}, sending task to workers", fn_name);
        let waiting_for_task_result_log_msg =
            format!("{{}}::Client::{}, waiting for task result", fn_name);

        let head = quote! {
            pub #client_signature -> impl ::std::future::Future<Output = #client_fn_output_type>
        };

        let task_struct_ident = self.function_name_as_pascal_case_ident("Params");

        let body = quote! {
            ::log::trace!(#sending_task_log_msg, module_path!());

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

                ::log::trace!(#waiting_for_task_result_log_msg, module_path!());

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

    fn as_client_execute_task_match_line(&self) -> TokenStream2 {
        let fn_name = self.0.ident.to_string();
        let task_variant_ident = self.function_name_as_pascal_case_ident("");

        let error_string_send = format!("{{}}::Client::{}, msg = {{:?}}", fn_name);
        let error_string_recv = format!("{{}}::Client::{} response, msg = {{:?}}", fn_name);

        let sending_task_log_msg = format!("{{}}::Client::{}, sending task to workers", fn_name);
        let waiting_for_task_result_log_msg =
            format!("{{}}::Client::{}, waiting for task result", fn_name);

        quote! {
            self::Task::#task_variant_ident(params) => {
                ::log::trace!(#sending_task_log_msg, module_path!());

                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                let ret = self.task_sender.send(self::ChanneledTask::#task_variant_ident { params, result_sender });
                if let Err(e) = ret {
                    ::log::error!(#error_string_send, module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                ::log::trace!(#waiting_for_task_result_log_msg, module_path!());
                match result_receiver.await {
                    Ok(ret) => Ok(self::TaskResult::#task_variant_ident(ret)),
                    Err(e) => {
                        ::log::error!(#error_string_recv, module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
        }
    }

    fn as_task_execution_match_line(
        &self,
        task_definitions_module_path: &syn::Path,
    ) -> TokenStream2 {
        let function_ident = &self.0.ident;
        let task_ident = self.function_name_as_pascal_case_ident("");
        let task_params_type_ident = self.function_name_as_pascal_case_ident("Params");
        let task_result_type_ident = self.function_name_as_pascal_case_ident("Result");

        let fn_parameter_names = self.inputs_as_parameter_names();

        let awaitness = if self.0.asyncness.is_some() {
            quote! {
                .await
            }
        } else {
            quote! {}
        };

        let executing_task_log_msg = format!(
            "Executing channeled task = {{}}::{}::{}",
            syn_path_to_string(task_definitions_module_path),
            task_ident,
        );

        quote! {
            #task_definitions_module_path::Task::#task_ident(
                #task_definitions_module_path::#task_params_type_ident {
                    #fn_parameter_names
                }
            ) => {
                ::log::trace!(#executing_task_log_msg, module_path!());
                let ret = self.#function_ident(#fn_parameter_names) #awaitness;
                #task_definitions_module_path::TaskResult::#task_ident(#task_definitions_module_path::#task_result_type_ident(ret))
            }
        }
    }

    fn as_channeled_task_execution_match_line(
        &self,
        task_definitions_module_path: &syn::Path,
    ) -> TokenStream2 {
        let function_ident = &self.0.ident;
        let task_ident = self.function_name_as_pascal_case_ident("");
        let task_params_type_ident = self.function_name_as_pascal_case_ident("Params");
        let task_result_type_ident = self.function_name_as_pascal_case_ident("Result");

        let fn_parameter_names = self.inputs_as_parameter_names();

        let awaitness = if self.0.asyncness.is_some() {
            quote! {
                .await
            }
        } else {
            quote! {}
        };

        let executing_channeled_task_log_msg = format!(
            "Executing channeled task = {{}}::{}::{}",
            syn_path_to_string(task_definitions_module_path),
            task_ident,
        );
        let sending_channeled_task_result_log_msg = format!(
            "Sending channeled task result = {{}}::{}::{}",
            syn_path_to_string(task_definitions_module_path),
            task_ident,
        );

        quote! {
            #task_definitions_module_path::ChanneledTask::#task_ident {
                result_sender,
                params: #task_definitions_module_path::#task_params_type_ident {
                    #fn_parameter_names
                }
            } => {
                ::log::trace!(#executing_channeled_task_log_msg, module_path!());
                let ret = self.#function_ident(#fn_parameter_names) #awaitness;

                ::log::trace!(#sending_channeled_task_result_log_msg, module_path!());
                let _ = result_sender.send(#task_definitions_module_path::#task_result_type_ident(ret));
            }
        }
    }
}

struct WorkerMethodSignatures(Vec<WorkerMethodSignature>);

impl WorkerMethodSignatures {
    fn has_async_method(&self) -> bool {
        self.0
            .iter()
            .any(|signature| signature.0.asyncness.is_some())
    }

    fn as_task_structs(&self, serde_derive: &TokenStream2) -> TokenStream2 {
        let mut ret = TokenStream2::new();

        ret.extend(self.0.iter().map(|item| item.as_task_structs(serde_derive)));

        ret
    }

    fn as_task_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(
            self.0
                .iter()
                .map(WorkerMethodSignature::as_task_enum_variant),
        );

        ret
    }

    fn as_task_result_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(
            self.0
                .iter()
                .map(WorkerMethodSignature::as_task_result_enum_variant),
        );

        ret
    }

    fn as_as_task_result_functions(&self) -> TokenStream2 {
        self.0
            .iter()
            .map(WorkerMethodSignature::as_as_task_result_funcion)
            .collect()
    }

    fn as_channeled_task_enum_variants(&self) -> Punctuated<TokenStream2, Comma> {
        let mut ret = Punctuated::<TokenStream2, Comma>::new();

        ret.extend(
            self.0
                .iter()
                .map(WorkerMethodSignature::as_channeled_task_enum_variant),
        );

        ret
    }

    fn task_builder_functions(&self) -> TokenStream2 {
        self.0
            .iter()
            .map(WorkerMethodSignature::as_task_builder_function)
            .collect()
    }

    fn as_client_impl<'a>(
        &self,
        client_name_ident: &syn::Ident,
        client_methods: impl Iterator<Item = &'a ClientMethod>,
    ) -> TokenStream2 {
        let client_functions: Vec<ClientFunction> = self
            .0
            .iter()
            .map(WorkerMethodSignature::as_client_function)
            .collect();

        let worker_fn_task_sender_definitions: TokenStream2 = client_functions
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

        let client_fn_definitions: TokenStream2 = client_methods
            .map(|item| {
                quote! {
                    #item
                }
            })
            .collect();

        let execute_task_match_lines: TokenStream2 = self
            .0
            .iter()
            .map(WorkerMethodSignature::as_client_execute_task_match_line)
            .collect();

        quote! {
            #[derive(Clone)]
            pub struct #client_name_ident {
                task_sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
            }

            impl #client_name_ident {
                pub fn new(
                    sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
                ) -> Self {
                    Self {
                        task_sender: sender,
                    }
                }

                pub async fn execute_task(
                    &self,
                    task: self::Task
                ) -> Result<self::TaskResult, ::method_taskifier::AllWorkersDroppedError> {
                    match task {
                        #execute_task_match_lines
                    }
                }

                #worker_fn_task_sender_definitions

                #client_fn_definitions
            }
        }
    }

    fn as_task_execution_match(&self, task_definitions_module_path: &syn::Path) -> TokenStream2 {
        let task_execution_match_lines: TokenStream2 = self
            .0
            .iter()
            .map(|signature| signature.as_task_execution_match_line(task_definitions_module_path))
            .collect();

        quote! {
            match task {
                #task_execution_match_lines
            }
        }
    }

    fn as_channeled_task_execution_match(
        &self,
        task_definitions_module_path: &syn::Path,
    ) -> TokenStream2 {
        let channeled_task_execution_match_lines: TokenStream2 = self
            .0
            .iter()
            .map(|signature| {
                signature.as_channeled_task_execution_match_line(task_definitions_module_path)
            })
            .collect();

        quote! {
            match task {
                #channeled_task_execution_match_lines
            }
        }
    }
}

struct ClientFunction {
    head: TokenStream2,
    body: TokenStream2,
}

struct WorkerTaskExecutorImpl {
    head: TokenStream2,
    body: TokenStream2,
}

impl WorkerTaskExecutorImpl {
    pub fn new(
        item_impl: &syn::ItemImpl,
        task_definitions_module_path: &syn::Path,
        fn_signatures: &WorkerMethodSignatures,
    ) -> Self {
        let attrs = &item_impl.attrs;
        let defaultness = &item_impl.defaultness;
        let unsafety = &item_impl.unsafety;
        let impl_token = &item_impl.impl_token;
        let generics = &item_impl.generics;
        let trait_ = &item_impl.trait_;
        let self_ty = &item_impl.self_ty;

        let has_async = fn_signatures.has_async_method();

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

        let task_execution_match =
            fn_signatures.as_task_execution_match(task_definitions_module_path);
        let channeled_task_execution_match =
            fn_signatures.as_channeled_task_execution_match(task_definitions_module_path);

        Self {
            head: quote! {
                #attributes_ts
                #defaultness
                #unsafety #impl_token #generics #trait_ts #self_ty
            },
            body: quote! {
                pub #executor_asyncness fn execute_task(
                    &mut self,
                    task: #task_definitions_module_path::Task,
                ) -> #task_definitions_module_path::TaskResult {
                    #task_execution_match
                }

                pub #executor_asyncness fn execute_channeled_task(
                    &mut self,
                    task: #task_definitions_module_path::ChanneledTask,
                ) {
                    #channeled_task_execution_match
                }

                pub #executor_asyncness fn try_execute_channeled_task_from_queue(
                    &mut self,
                    receiver: &::method_taskifier::task_channel::TaskReceiver<#task_definitions_module_path::ChanneledTask>,
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
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<#task_definitions_module_path::ChanneledTask>,
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
                    receiver: &::method_taskifier::task_channel::TaskReceiver<#task_definitions_module_path::ChanneledTask>,
                ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
                    while self.try_execute_channeled_task_from_queue(receiver) #executor_call_awaitness ? {}
                    Ok(())
                }

                pub async fn execute_channeled_tasks_from_queue_until_clients_dropped(
                    &mut self,
                    receiver: &mut ::method_taskifier::task_channel::TaskReceiver<#task_definitions_module_path::ChanneledTask>,
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

struct MethodTaskifierImpl {
    worker_impl: syn::ItemImpl,
    worker_method_signatures: WorkerMethodSignatures,
    client_methods: Vec<ClientMethod>,
}

impl Parse for MethodTaskifierImpl {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut worker_impl: syn::ItemImpl = input.parse()?;

        let worker_method_signatures = WorkerMethodSignatures(
            worker_impl
                .items
                .iter_mut()
                .filter_map(|item| {
                    if let syn::ImplItem::Fn(method) = item {
                        let mut found_attribute = false;
                        method.attrs.retain(|attr| {
                            if does_attribute_equals_to(attr, "method_taskifier_worker_fn") {
                                found_attribute = true;
                                false
                            } else {
                                true
                            }
                        });

                        if found_attribute {
                            Some(WorkerMethodSignature(method.sig.clone()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect(),
        );

        let mut client_methods = Vec::new();
        worker_impl.items.retain_mut(|item| {
            if let syn::ImplItem::Fn(method) = item {
                let mut found_attribute = false;
                method.attrs.retain(|attr| {
                    if does_attribute_equals_to(attr, "method_taskifier_client_fn") {
                        found_attribute = true;
                        false
                    } else {
                        true
                    }
                });

                if found_attribute {
                    client_methods.push(ClientMethod {
                        sig: method.sig.clone(),
                        body: method.block.clone(),
                    });

                    false
                } else {
                    true
                }
            } else {
                true
            }
        });

        Ok(Self {
            worker_impl,
            worker_method_signatures,
            client_methods,
        })
    }
}

#[proc_macro_attribute]
pub fn method_taskifier_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let method_traskifier_impl_args: MethodTaskifierImplArgs = syn::parse_macro_input!(args);
    let method_taskifier_impl: MethodTaskifierImpl = syn::parse_macro_input!(input);

    let serde_derive = if method_traskifier_impl_args.use_serde {
        quote! {
            #[derive(::serde::Serialize, ::serde::Deserialize)]
        }
    } else {
        quote! {}
    };

    let task_definitions_module_path = &method_traskifier_impl_args.task_definitions_module_path;
    let client_name_ident = &method_traskifier_impl_args.client_name;
    let worker_task_executor_impl = WorkerTaskExecutorImpl::new(
        &method_taskifier_impl.worker_impl,
        task_definitions_module_path,
        &method_taskifier_impl.worker_method_signatures,
    );
    let task_structs: TokenStream2 = method_taskifier_impl
        .worker_method_signatures
        .as_task_structs(&serde_derive);
    let task_declarations: Punctuated<TokenStream2, Comma> = method_taskifier_impl
        .worker_method_signatures
        .as_task_enum_variants();
    let task_result_declarations: Punctuated<TokenStream2, Comma> = method_taskifier_impl
        .worker_method_signatures
        .as_task_result_enum_variants();
    let as_task_result_functions = method_taskifier_impl
        .worker_method_signatures
        .as_as_task_result_functions();
    let channeled_task_declarations: Punctuated<TokenStream2, Comma> = method_taskifier_impl
        .worker_method_signatures
        .as_channeled_task_enum_variants();
    let client_impl = method_taskifier_impl
        .worker_method_signatures
        .as_client_impl(
            client_name_ident,
            method_taskifier_impl.client_methods.iter(),
        );
    let task_builder_functions = method_taskifier_impl
        .worker_method_signatures
        .task_builder_functions();

    let item_impl = &method_taskifier_impl.worker_impl;

    let tokenstream = quote! {
        #item_impl

        #worker_task_executor_impl

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
    };

    if method_traskifier_impl_args.debug_mode {
        panic!("{tokenstream}");
    }

    tokenstream.into()
}
