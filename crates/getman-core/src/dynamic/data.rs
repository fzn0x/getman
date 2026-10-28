//! Word lists for the dynamic values, written for Getman. Each list is one
//! string of comma-separated entries, so a list stays a few lines long.

/// One entry of a comma-separated list, chosen at random.
pub(super) fn choose(list: &'static str) -> &'static str {
    let count = list.split(',').count();
    list.split(',')
        .nth(fastrand::usize(..count))
        .unwrap_or_default()
}

pub(super) const NAME_PREFIXES: &str = "Mr,Mrs,Ms,Miss,Dr,Prof,Mx";
pub(super) const NAME_SUFFIXES: &str = "Jr,Sr,II,III,IV,PhD,MD,DDS,Esq";

pub(super) const STREET_KINDS: &str = "Street,Avenue,Road,Lane,Drive,Court,Way,Terrace,Boulevard,\
    Square,Place,Crescent";

pub(super) const JOB_AREAS: &str = "Billing,Catalog,Compliance,Delivery,Finance,Fleet,Growth,\
    Hiring,Insights,Logistics,Onboarding,Partnerships,Payroll,Procurement,Reliability,Retention,\
    Sourcing,Support,Training,Warehouse";
pub(super) const JOB_DESCRIPTORS: &str = "Acting,Assistant,Associate,Chief,Executive,Founding,\
    Junior,Lead,Principal,Regional,Senior,Staff,Visiting";
pub(super) const JOB_TYPES: &str = "Analyst,Architect,Auditor,Buyer,Coach,Controller,Coordinator,\
    Designer,Editor,Engineer,Inspector,Manager,Planner,Researcher,Scheduler,Specialist,\
    Strategist,Technician,Writer";

pub(super) const COMPANY_SUFFIXES: &str = "Inc,LLC,Ltd,Group,Partners,Co,Holdings,Works,Labs,\
    Collective";

pub(super) const BS_ADJECTIVES: &str = "audited,bundled,cloud-ready,cost-aware,data-rich,\
    hands-on,low-touch,measurable,mobile-first,on-demand,outcome-based,partner-led,self-serve,\
    subscription,tiered,zero-waste";
pub(super) const BS_BUZZ: &str = "align,amplify,bootstrap,consolidate,curate,democratize,disrupt,\
    elevate,incubate,monetize,onboard,pivot,prototype,rebrand,right-size,scale,unlock,upskill";
pub(super) const BS_NOUNS: &str = "action items,workflows,roadmaps,touchpoints,stakeholders,\
    verticals,pipelines,playbooks,cohorts,funnels,bandwidth,ecosystems,learnings,mindshare,\
    value streams,use cases";

pub(super) const CATCH_ADJECTIVES: &str = "Affordable,Battle-tested,Boundless,Cloud-native,\
    Convenient,Dependable,Effortless,Everyday,Flexible,Hassle-free,Intuitive,Reliable,Responsive,\
    Smarter,Trusted,Unbeatable";
pub(super) const CATCH_DESCRIPTORS: &str = "always-on,award-winning,built-in,community-backed,\
    customer-first,easy-to-use,fully-managed,future-proof,high-availability,human-centered,\
    low-latency,open-ended,plug-and-play,privacy-focused,round-the-clock,state-of-the-art";
pub(super) const CATCH_NOUNS: &str = "dashboard,platform,toolkit,gateway,workspace,network,\
    engine,suite,studio,framework,pipeline,service,backbone,console,hub,assistant";

pub(super) const ABBREVIATIONS: &str = "ACL,API,AVI,CDN,CPU,CSV,DNS,GPU,GUI,HDD,HTTP,IDE,IP,JSON,\
    JWT,LAN,OCR,PCI,RAM,REST,RSS,SAS,SCSI,SDK,SFTP,SMS,SMTP,SQL,SSD,SSH,SSL,TCP,TLS,UDP,URL,USB,\
    VPN,XML,YAML";

pub(super) const LOCALES: &str = "af,ar,bg,ca,cs,da,de,el,en,en-GB,en-US,es,es-MX,et,fa,fi,fr,\
    fr-CA,he,hi,hr,hu,id,it,ja,ko,lt,lv,ms,nb,nl,pl,pt,pt-BR,ro,ru,sk,sl,sr,sv,th,tr,uk,vi,zh-CN,\
    zh-TW";

pub(super) const DOMAIN_SUFFIXES: &str = "com,net,org,io,dev,app,info,biz,co,xyz,tech,cloud";
pub(super) const EXAMPLE_DOMAINS: &str = "example.com,example.net,example.org";

pub(super) const WEEKDAYS: &str = "Monday,Tuesday,Wednesday,Thursday,Friday,Saturday,Sunday";
pub(super) const MONTHS: &str = "January,February,March,April,May,June,July,August,September,\
    October,November,December";

/// Currency code, name and symbol, separated by `|`.
pub(super) const CURRENCIES: &str = "AED|UAE Dirham|د.إ,ARS|Argentine Peso|$,\
    AUD|Australian Dollar|$,BRL|Brazilian Real|R$,CAD|Canadian Dollar|$,CHF|Swiss Franc|CHF,\
    CNY|Chinese Yuan|¥,CZK|Czech Koruna|Kč,DKK|Danish Krone|kr,EGP|Egyptian Pound|£,EUR|Euro|€,\
    GBP|Pound Sterling|£,HKD|Hong Kong Dollar|$,IDR|Indonesian Rupiah|Rp,ILS|Israeli Shekel|₪,\
    INR|Indian Rupee|₹,JPY|Japanese Yen|¥,KES|Kenyan Shilling|KSh,KRW|South Korean Won|₩,\
    MXN|Mexican Peso|$,NGN|Nigerian Naira|₦,NOK|Norwegian Krone|kr,NZD|New Zealand Dollar|$,\
    PLN|Polish Zloty|zł,SEK|Swedish Krona|kr,SGD|Singapore Dollar|$,THB|Thai Baht|฿,\
    TRY|Turkish Lira|₺,USD|US Dollar|$,VND|Vietnamese Dong|₫,ZAR|South African Rand|R";

pub(super) const ACCOUNT_NAMES: &str = "Checking Account,Savings Account,Money Market Account,\
    Investment Account,Household Account,Joint Account,Business Account,Payroll Account";
pub(super) const TRANSACTION_TYPES: &str = "deposit,withdrawal,payment,invoice,refund,transfer";

pub(super) const DATABASE_COLUMNS: &str = "id,uuid,slug,label,username,email_address,\
    phone_number,api_key,role,tag,description,avatar_url,state,created_on,updated_on,deleted_on,\
    owner_id,balance_due";
pub(super) const DATABASE_TYPES: &str = "integer,bigint,varchar,char,text,boolean,decimal,float,\
    date,time,timestamp,datetime,json,uuid,blob";
pub(super) const DATABASE_COLLATIONS: &str = "utf8_general_ci,utf8_bin,utf8mb4_unicode_ci,\
    latin1_swedish_ci,ascii_general_ci,cp1250_general_ci,binary";
pub(super) const DATABASE_ENGINES: &str = "InnoDB,MyISAM,MEMORY,ARCHIVE,CSV,BLACKHOLE,NDB,Aria,\
    RocksDB";

pub(super) const DEPARTMENTS: &str = "Appliances,Art Supplies,Bakery,Bedding,Cameras,Cleaning,\
    Cookware,Footwear,Furniture,Hardware,Lighting,Luggage,Office,Pharmacy,Produce,Stationery,\
    Travel,Wine";
pub(super) const PRODUCT_ADJECTIVES: &str = "Compact,Durable,Elegant,Ergonomic,Foldable,Handmade,\
    Lightweight,Modern,Portable,Practical,Refined,Rustic,Sleek,Sturdy,Waterproof";
pub(super) const PRODUCT_MATERIALS: &str = "Bamboo,Bronze,Ceramic,Clay,Cork,Copper,Glass,Hemp,\
    Leather,Linen,Marble,Oak,Pewter,Silk,Slate,Teak,Wool";
pub(super) const PRODUCTS: &str = "Backpack,Blender,Bookshelf,Candle,Coat,Colander,Drill,Hammock,\
    Jacket,Kettle,Mug,Notebook,Rug,Scarf,Skillet,Speaker,Sofa,Teapot,Umbrella,Vase";

pub(super) const NOUNS: &str = "answer,bridge,camera,channel,circuit,compass,context,counter,\
    diagram,engine,factory,folder,gateway,handle,journal,ledger,machine,marker,network,notebook,\
    packet,pattern,pipeline,plugin,process,queue,record,register,schema,signal,socket,stream,\
    switch,ticket,toolbox,window";
pub(super) const VERBS: &str = "archive,bundle,calibrate,catalog,clone,compile,deploy,dispatch,\
    encode,fetch,inspect,merge,monitor,produce,query,render,restart,restore,route,scan,sync,\
    tokenize,traverse,validate,verify,attach";
pub(super) const ING_VERBS: &str = "archiving,bundling,calibrating,cataloging,cloning,compiling,\
    deploying,dispatching,encoding,fetching,inspecting,merging,monitoring,producing,querying,\
    rendering,restarting,restoring,routing,scanning,syncing,tokenizing,traversing,validating,\
    verifying,attaching";
pub(super) const ADJECTIVES: &str = "agile,bright,calm,clever,compact,distant,eager,fluent,\
    gentle,golden,hidden,honest,keen,lively,mellow,narrow,nimble,patient,quiet,rapid,silent,\
    smooth,steady,subtle,tidy,vivid,warm,wide";

/// Sentence shapes for `$randomPhrase`: `{v}` is a verb, `{n}` a noun and
/// `{a}` an adjective.
pub(super) const PHRASES: &str = "We should {v} the {a} {n}.,\
    Try to {v} the {n} and then {v} the {a} {n}.,If we {v} the {n} the {a} {n} will follow.,\
    I will {v} the {a} {n} before lunch.,You can't {v} the {n} without the {a} {n}.,\
    Let's {v} the {n} and see what happens.,The {a} {n} needs a {n} to {v}.,\
    Nobody wants to {v} the {a} {n}.";

pub(super) const LOREM_WORDS: &str = "lorem,ipsum,dolor,sit,amet,consectetur,adipiscing,elit,sed,\
    do,eiusmod,tempor,incididunt,ut,labore,et,dolore,magna,aliqua,enim,ad,minim,veniam,quis,\
    nostrud,exercitation,ullamco,laboris,nisi,aliquip,ex,ea,commodo,consequat,duis,aute,irure,in,\
    reprehenderit,voluptate,velit,esse,cillum,eu,fugiat,nulla,pariatur,excepteur,sint,occaecat,\
    cupidatat,non,proident,sunt,culpa,qui,officia,deserunt,mollit,anim,id,est,laborum,vitae,\
    tellus,mauris,posuere,integer,urna,pretium,felis,nec,rutrum,quam,ligula,faucibus";

/// File extension, MIME type and kind, separated by `|`.
pub(super) const FILE_TYPES: &str = "7z|application/x-7z-compressed|application,\
    avi|video/x-msvideo|video,bmp|image/bmp|image,css|text/css|text,csv|text/csv|text,\
    doc|application/msword|application,flac|audio/flac|audio,gif|image/gif|image,\
    gz|application/gzip|application,html|text/html|text,ics|text/calendar|text,\
    jpeg|image/jpeg|image,js|text/javascript|text,json|application/json|application,\
    md|text/markdown|text,mov|video/quicktime|video,mp3|audio/mpeg|audio,mp4|video/mp4|video,\
    ogg|audio/ogg|audio,pdf|application/pdf|application,png|image/png|image,\
    ppt|application/vnd.ms-powerpoint|application,svg|image/svg+xml|image,\
    tar|application/x-tar|application,txt|text/plain|text,wav|audio/wav|audio,\
    webm|video/webm|video,webp|image/webp|image,xls|application/vnd.ms-excel|application,\
    xml|application/xml|application,yaml|application/yaml|application,\
    zip|application/zip|application";
/// The extensions of [`FILE_TYPES`] that people meet every day.
pub(super) const COMMON_EXTENSIONS: &str = "csv,gif,html,jpeg,json,mp3,mp4,pdf,png,svg,txt,xml,\
    zip";
pub(super) const DIRECTORIES: &str = "/etc/mail,/home/user/documents,/opt/tools,/srv/data,\
    /usr/local/share,/var/log,/var/lib/cache,/tmp/uploads,/mnt/storage/backups";

pub(super) const IMAGE_KINDS: &str = "abstract,animals,business,cats,city,food,nightlife,fashion,\
    people,nature,sports,technics,transport";

pub(super) const USER_AGENT_PLATFORMS: &str = "Windows NT 10.0; Win64; x64,\
    Macintosh; Intel Mac OS X 10_15_7,X11; Linux x86_64,X11; Ubuntu; Linux x86_64,\
    Linux; Android 14; Pixel 8,iPhone; CPU iPhone OS 17_5 like Mac OS X";
