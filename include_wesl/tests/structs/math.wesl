@if(half)
enable f16;

@if(half)
alias m16 = f16;

@if(!half)
alias m16 = f32;
