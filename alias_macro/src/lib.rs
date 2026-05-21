use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Block, visit::Visit, Expr, ExprMethodCall, ExprCast,
    ExprPath, Item, Macro, ExprReference, Type, punctuated::Punctuated,
    Token, Ident,
};
use syn::parse::Parser;

enum Violation {
    RawAddr(proc_macro2::Span, bool),
    CastToPtr(proc_macro2::Span, bool),
    PtrFunction(proc_macro2::Span, bool),
    MethodCall(proc_macro2::Span, String),
    DerefToRef(proc_macro2::Span, bool),
    //DirectDeref(proc_macro2::Span),
    SharedRef(proc_macro2::Span),
    MutRef(proc_macro2::Span),
}

impl Violation {
    fn span(&self) -> proc_macro2::Span {
        match self {
            Violation::RawAddr(s, _) => *s,
            Violation::CastToPtr(s, _) => *s,
            Violation::PtrFunction(s, _) => *s,
            Violation::MethodCall(s, _) => *s,
            Violation::DerefToRef(s, _) => *s,
          //  Violation::DirectDeref(s) => *s,
            Violation::SharedRef(s)     => *s,
            Violation::MutRef(s)        => *s,
        }
    }

    fn message(&self) -> String {
        match self {
            Violation::RawAddr(_, is_mut) => {
                if *is_mut {
"creating mutable raw pointer using `&raw` within this guard is forbidden.\n\n\
Use:\n `guard!(guard_name *mut any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                } else {
                    "creating immutable raw pointer using `&raw` within this guard is forbidden. \n\n\
Use:\n `guard!(guard_name *const any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                }
            }
            Violation::CastToPtr(_, is_mut) => {
                if *is_mut {
                    "casting to mutable raw pointer using `as` is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *mut any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                } else {
                    "casting to an immutable raw pointer using `as` is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *const any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                }
            }
            Violation::PtrFunction(_, is_mut) => {
                if *is_mut {
                    "creating mutable raw pointer via standard pointer functions is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *mut any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                } else {
                    "creating immutable raw pointer via standard pointer functions is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *const any_name {\n\n\
    // println!(\"{}\", *any_name);\n\n\
});` instead\n".into()
                }
            }
            Violation::MethodCall(_, method) => {
                let is_mut = method.contains("mut");
                if is_mut {
                    format!("calling `{}` to create or manage direct mutable raw pointer is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *mut any_name {{\n\n\
    // println!(\"{{}}\", *any_name);\n\n\
}});` instead\n", method)
                } else {
                    format!("calling `{}` to create or manage direct immutable raw pointer is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name *const any_name {{\n\n\
    // println!(\"{{}}\", *any_name);\n\n\
}});` instead\n", method)
                }
            }
            Violation::DerefToRef(_, is_mut) => {
                if *is_mut {
                    "converting raw pointer back into mutable reference via dereferencing is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name &mut any_name {{\n\n\
    // println!(\"{{}}\", *any_name);\n\n\
}});` instead\n".into()
                } else {
                    "converting raw pointer back into immutable reference via dereferencing is forbidden within this guard. \n\n\
Use:\n `guard!(guard_name &any_name {{\n\n\
    // println!(\"{{}}\", *any_name);\n\n\
}});` instead\n".into()
                }
            }
            Violation::SharedRef(_) => {
                "creating a shared reference `&` within this guard is forbidden.\n\n\
Only raw pointers (`*const T` / `*mut T`) are allowed here.\n".into()
            }
            Violation::MutRef(_) => {
                "creating a mutable reference `&mut` within this guard is forbidden.\n\n\
Only raw pointers (`*const T` / `*mut T`) are allowed here.\n".into()
            } 
            
            
        }
    }
}

fn is_forbidden_ptr_function(segments: &[String]) -> Option<bool> {
    let last = segments.last().map(|s| s.as_str())?;
    let joined = segments.join("::");

    let is_guard_constructor = joined.contains("AliasingGuardMut");
    if is_guard_constructor {
        return None;
    }

    match last {
        "from_ref" => Some(true),
        "slice_from_raw_parts" => Some(true),
        "null" => Some(false),
        "invalid" => Some(false),
        "dangling" => Some(false),
        "from_mut" => Some(true),
        "from_raw" => Some(true),
        "null_mut" => Some(true),
        "invalid_mut" => Some(true),
        _ => None,
    }
}

fn is_forbidden_method(name: &str) -> bool {
    matches!(name,
        "as_ptr" | "as_mut_ptr" | "as_ref" | "as_mut" | "as_slice_of_cells"
    )
}

struct Detector {
    violations: Vec<Violation>,
    accessed_idents: Vec<Ident>,
    declared_idents: Vec<Ident>,
}

impl Detector {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            accessed_idents: Vec::new(),
            declared_idents: Vec::new(),
        }
    }

    fn push(&mut self, v: Violation) {
        self.violations.push(v);
    }
}

impl<'ast> Visit<'ast> for Detector {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.declared_idents.push(node.ident.clone());
        syn::visit::visit_pat_ident(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if node.path.leading_colon.is_none() && node.path.segments.len() == 1 {
            let ident = &node.path.segments[0].ident;
            let name = ident.to_string();
            if name != "unsafe"
                && !self.accessed_idents.contains(ident)
                && !self.declared_idents.contains(ident)
            {
                self.accessed_idents.push(ident.clone());
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        use syn::spanned::Spanned;

        match node {
            Expr::Macro(expr_macro) => {
                self.visit_macro(&expr_macro.mac);
            }
            Expr::RawAddr(expr_raw_addr) => {
                let is_mut = matches!(expr_raw_addr.mutability, syn::PointerMutability::Mut(_));
                self.push(Violation::RawAddr(node.span(), is_mut));
            }
            Expr::Cast(ExprCast { ty, .. }) => {
                if let Type::Ptr(type_ptr) = &**ty {
                    let is_mut = type_ptr.mutability.is_some();
                    self.push(Violation::CastToPtr(node.span(), is_mut));
                }
            }
            Expr::Call(expr_call) => {
                if let Expr::Path(ExprPath { path, .. }) = &*expr_call.func {
                    let segments: Vec<String> = path.segments.iter()
                        .map(|s| s.ident.to_string())
                        .collect();

                    if let Some(is_mut) = is_forbidden_ptr_function(&segments) {
                        self.push(Violation::PtrFunction(node.span(), is_mut));
                    }
                }
            }
            Expr::Reference(ExprReference { expr, mutability, .. }) => {
                if let Expr::Unary(syn::ExprUnary { op: syn::UnOp::Deref(_), .. }) = &**expr {
                    let is_mut = mutability.is_some();
                    self.push(Violation::DerefToRef(node.span(), is_mut));
                }
            }
            /*
            Expr::Unary(syn::ExprUnary { op: syn::UnOp::Deref(_), .. }) => {
                self.push(Violation::DirectDeref(node.span()));
            }
            */
            _ => {}
        }

        syn::visit::visit_expr(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        use syn::spanned::Spanned;
        let name = node.method.to_string();

        if is_forbidden_method(&name) {
            self.push(Violation::MethodCall(node.span(), name));
        }

        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let tokens: proc_macro2::TokenStream = node.tokens.clone();

        if let Ok(block) = syn::parse2::<syn::Block>(quote!({ #tokens })) {
            self.visit_block(&block);
        } else if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expr);
        } else {
            let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
            if let Ok(exprs) = parser.parse2(tokens) {
                for expr in exprs {
                    self.visit_expr(&expr);
                }
            }
        }

        syn::visit::visit_macro(self, node);
    }
}

fn violations_to_errors(violations: Vec<Violation>) -> proc_macro2::TokenStream {
    let mut errors = proc_macro2::TokenStream::new();
    for v in violations {
        errors.extend(syn::Error::new(v.span(), v.message()).into_compile_error());
    }
    errors
}

#[proc_macro]
pub fn guard_block(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    let block_tokens: proc_macro2::TokenStream = {
        let parser = |stream: syn::parse::ParseStream| -> syn::Result<proc_macro2::TokenStream> {
            let content;
            syn::braced!(content in stream);
            content.parse::<proc_macro2::TokenStream>()
        };
        match syn::parse::Parser::parse2(parser, input2) {
            Ok(t) => t,
            Err(e) => return e.into_compile_error().into(),
        }
    };

    let block: Block = match syn::parse2(quote!({ #block_tokens })) {
        Ok(b) => b,
        Err(e) => return e.into_compile_error().into(),
    };

    let mut d = Detector::new();
    d.visit_block(&block);

    let error_tokens = violations_to_errors(d.violations);

    let idents: Vec<_> = d.accessed_idents
        .iter()
        .filter(|id| !d.declared_idents.contains(id))
        .cloned()
        .collect();

    quote!({
        #error_tokens

        trait NormalType { fn forbid_raw_pointer(&self) {} }
        impl<T> NormalType for T {}

        struct PtrDetector<T>(T);
        impl<T> PtrDetector<*const T> { fn forbid_raw_pointer(&self) {} }
        impl<T> PtrDetector<*mut T> { fn forbid_raw_pointer(&self) {} }

        
        #(
            PtrDetector(&#idents).forbid_raw_pointer();
        )*
        
        
        #(
            ::aliasing_guard::assert_no_pointer_val(&#idents);
        )*
        

        #block_tokens
    }).into()
}

#[proc_macro_attribute]
pub fn guard(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_item = parse_macro_input!(item as Item);

    let mut d = Detector::new();
    d.visit_item(&input_item);

    let error_tokens = violations_to_errors(d.violations);

    quote!(
        #error_tokens
        #input_item
    ).into()
}



struct DetectorNoRef {
    violations: Vec<Violation>,
    accessed_idents: Vec<Ident>,
    declared_idents: Vec<Ident>,
}

impl DetectorNoRef {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            accessed_idents: Vec::new(),
            declared_idents: Vec::new(),
        }
    }
    fn push(&mut self, v: Violation) { self.violations.push(v); }
}

impl<'ast> Visit<'ast> for DetectorNoRef {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.declared_idents.push(node.ident.clone());
        syn::visit::visit_pat_ident(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if node.path.leading_colon.is_none() && node.path.segments.len() == 1 {
            let ident = &node.path.segments[0].ident;
            let name = ident.to_string();
            if name != "unsafe"
                && !self.accessed_idents.contains(ident)
                && !self.declared_idents.contains(ident)
            {
                self.accessed_idents.push(ident.clone());
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        use syn::spanned::Spanned;
        match node {
            Expr::Macro(expr_macro) => { self.visit_macro(&expr_macro.mac); }

            Expr::Reference(ExprReference { mutability, .. }) => {
                if mutability.is_some() {
                    self.push(Violation::MutRef(node.span()));
                } else {
                    self.push(Violation::SharedRef(node.span()));
                }
            }
            _ => {}
        }
        syn::visit::visit_expr(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {

        use syn::spanned::Spanned;
        let name = node.method.to_string();
        if matches!(name.as_str(), "as_ref" | "as_mut") {
            let is_mut = name == "as_mut";
            if is_mut {
                self.push(Violation::MutRef(node.span()));
            } else {
                self.push(Violation::SharedRef(node.span()));
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let tokens: proc_macro2::TokenStream = node.tokens.clone();
        if let Ok(block) = syn::parse2::<syn::Block>(quote!({ #tokens })) {
            self.visit_block(&block);
        } else if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expr);
        } else {
            let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
            if let Ok(exprs) = parser.parse2(tokens) {
                for expr in exprs { self.visit_expr(&expr); }
            }
        }
        syn::visit::visit_macro(self, node);
    }
}

#[proc_macro]
pub fn guard_block_no_reference(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    let block_tokens: proc_macro2::TokenStream = {
        let parser = |stream: syn::parse::ParseStream| -> syn::Result<proc_macro2::TokenStream> {
            let content;
            syn::braced!(content in stream);
            content.parse::<proc_macro2::TokenStream>()
        };
        match syn::parse::Parser::parse2(parser, input2) {
            Ok(t) => t,
            Err(e) => return e.into_compile_error().into(),
        }
    };

    let block: Block = match syn::parse2(quote!({ #block_tokens })) {
        Ok(b) => b,
        Err(e) => return e.into_compile_error().into(),
    };

    let mut d = DetectorNoRef::new();
    d.visit_block(&block);
    let error_tokens = violations_to_errors(d.violations);

    quote!({
        #error_tokens
        #block_tokens
    }).into()
}

#[proc_macro_attribute]
pub fn guard_no_reference(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_item = parse_macro_input!(item as Item);
    let mut d = DetectorNoRef::new();
    d.visit_item(&input_item);
    let error_tokens = violations_to_errors(d.violations);
    quote!(
        #error_tokens
        #input_item
    ).into()
}


struct DetectorNoWrite {
    violations: Vec<Violation>,
    accessed_idents: Vec<Ident>,
    declared_idents: Vec<Ident>,
}

impl DetectorNoWrite {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            accessed_idents: Vec::new(),
            declared_idents: Vec::new(),
        }
    }
    fn push(&mut self, v: Violation) { self.violations.push(v); }
}

impl<'ast> Visit<'ast> for DetectorNoWrite {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.declared_idents.push(node.ident.clone());
        syn::visit::visit_pat_ident(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if node.path.leading_colon.is_none() && node.path.segments.len() == 1 {
            let ident = &node.path.segments[0].ident;
            let name = ident.to_string();
            if name != "unsafe"
                && !self.accessed_idents.contains(ident)
                && !self.declared_idents.contains(ident)
            {
                self.accessed_idents.push(ident.clone());
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        use syn::spanned::Spanned;
        match node {
            Expr::Macro(expr_macro) => { self.visit_macro(&expr_macro.mac); }
            Expr::RawAddr(expr_raw_addr) => {
                if matches!(expr_raw_addr.mutability, syn::PointerMutability::Mut(_)) {
                    self.push(Violation::RawAddr(node.span(), true));
                }
            }
            Expr::Cast(ExprCast { ty, .. }) => {
                if let Type::Ptr(type_ptr) = &**ty {
                    if type_ptr.mutability.is_some() {
                        self.push(Violation::CastToPtr(node.span(), true));
                    }
                }
            }
            Expr::Call(expr_call) => {
                if let Expr::Path(ExprPath { path, .. }) = &*expr_call.func {
                    let segments: Vec<String> = path.segments.iter()
                        .map(|s| s.ident.to_string())
                        .collect();
                    if let Some(is_mut) = is_forbidden_ptr_function(&segments) {
                        if is_mut {
                            self.push(Violation::PtrFunction(node.span(), true));
                        }
                    }
                }
            }
            Expr::Reference(ExprReference { mutability, .. }) => {
                if mutability.is_some() {
                    self.push(Violation::MutRef(node.span()));
                }
            }
            _ => {}
        }
        syn::visit::visit_expr(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        use syn::spanned::Spanned;
        let name = node.method.to_string();
        if name == "as_mut" || name == "as_mut_ptr" {
            self.push(Violation::MutRef(node.span()));
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let tokens: proc_macro2::TokenStream = node.tokens.clone();
        if let Ok(block) = syn::parse2::<syn::Block>(quote!({ #tokens })) {
            self.visit_block(&block);
        } else if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expr);
        } else {
            let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
            if let Ok(exprs) = parser.parse2(tokens) {
                for expr in exprs { self.visit_expr(&expr); }
            }
        }
        syn::visit::visit_macro(self, node);
    }
}

#[proc_macro]
pub fn guard_block_no_write(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    let block_tokens: proc_macro2::TokenStream = {
        let parser = |stream: syn::parse::ParseStream| -> syn::Result<proc_macro2::TokenStream> {
            let content;
            syn::braced!(content in stream);
            content.parse::<proc_macro2::TokenStream>()
        };
        match syn::parse::Parser::parse2(parser, input2) {
            Ok(t) => t,
            Err(e) => return e.into_compile_error().into(),
        }
    };

    let block: Block = match syn::parse2(quote!({ #block_tokens })) {
        Ok(b) => b,
        Err(e) => return e.into_compile_error().into(),
    };

    let mut d = DetectorNoWrite::new();
    d.visit_block(&block);
    let error_tokens = violations_to_errors(d.violations);

    let idents: Vec<_> = d.accessed_idents
        .iter()
        .filter(|id| !d.declared_idents.contains(id))
        .cloned()
        .collect();

    quote!({
        #error_tokens

        trait AllowedWriteGuardType { fn assert_no_write(&self) {} }
        impl<T> AllowedWriteGuardType for T {}

        struct WriteDetector<T>(T);
        impl<T> WriteDetector<*mut T> { fn assert_no_write(&self) {} }
        impl<T> WriteDetector<&mut T> { fn assert_no_write(&self) {} }

        #(
            WriteDetector(&#idents).assert_no_write();
        )*
        
        #(
            ::aliasing_guard::assert_no_pointer_val(&#idents);
        )*

        #block_tokens
    }).into()
}

#[proc_macro_attribute]
pub fn guard_no_write(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_item = parse_macro_input!(item as Item);
    let mut d = DetectorNoWrite::new();
    d.visit_item(&input_item);
    let error_tokens = violations_to_errors(d.violations);
    quote!(
        #error_tokens
        #input_item
    ).into()
}